use crate::{ui::*, AppError, Result};
use serde_json::Value;

/// The original server-saved ui-layout.json, normalized like columnPrefs and
/// normalizeUILayout. Browser-profile/localStorage bytes remain untouched.
pub fn adapt(value: &Value) -> Result<UiState> {
    if value.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(AppError::new(
            "legacy-ui-layout",
            "Unknown original layout schema",
        ));
    }
    let mut state = UiState::default();
    let mut presentation = PresentationState::default();
    if let Some(ratio) = value
        .get("split_ratio")
        .and_then(Value::as_f64)
        .filter(|v| (1.0..=99.0).contains(v))
    {
        presentation.split_basis_points = ratio * 100.0;
    }
    if let Some(task) = value.get("task_center") {
        presentation.task_open = task.get("open").and_then(Value::as_bool) == Some(true);
        if let Some(height) = task
            .get("cli_height_px")
            .and_then(Value::as_f64)
            .filter(|v| (190.0..=4000.0).contains(v))
        {
            presentation.task_height = height;
        }
    }
    for (name, view, target) in [
        ("movies", &mut state.movie, &mut presentation.movie_columns),
        ("tv", &mut state.tv, &mut presentation.tv_columns),
    ] {
        let Some(source) = value.get("catalog").and_then(|v| v.get(name)) else {
            continue;
        };
        let mut columns = CatalogColumns::default();
        if let Some(saved) = source.get("columns").filter(|v| v.is_object()) {
            let read = |key: &str| -> Option<Vec<CatalogColumn>> {
                saved.get(key).and_then(Value::as_array).map(|values| {
                    let mut result = vec![];
                    for value in values {
                        if let Ok(column) = serde_json::from_value::<CatalogColumn>(value.clone()) {
                            if !result.contains(&column) {
                                result.push(column);
                            }
                        }
                    }
                    result
                })
            };
            if let Some(mut order) = read("order") {
                for column in &columns.order {
                    if !order.contains(column) {
                        order.push(column.clone());
                    }
                }
                order.retain(|v| *v != CatalogColumn::Title);
                order.insert(0, CatalogColumn::Title);
                columns.order = order;
            }
            if let Some(mut visible) = read("visible") {
                if !visible.contains(&CatalogColumn::Title) {
                    visible.insert(0, CatalogColumn::Title);
                }
                columns.visible = visible;
            }
            columns.compact = saved.get("compact").and_then(Value::as_bool) == Some(true);
            if saved.get("widthMode").and_then(Value::as_str) == Some("pixels-current") {
                if let Some(widths) = saved.get("widths").and_then(Value::as_object) {
                    for (key, value) in widths {
                        if let (Ok(column), Some(width)) = (
                            serde_json::from_value::<CatalogColumn>(Value::String(key.clone())),
                            value.as_f64().filter(|v| (1.0..=4000.0).contains(v)),
                        ) {
                            columns.widths.insert(column, width.round() as u16);
                        }
                    }
                }
            }
        }
        let sort = source.get("sort");
        let field = sort
            .and_then(|v| v.get("field"))
            .cloned()
            .unwrap_or(Value::Null);
        let column = serde_json::from_value::<CatalogColumn>(field)
            .ok()
            .filter(|c| columns.visible.contains(c))
            .unwrap_or(CatalogColumn::Title);
        view.sort = match column {
            CatalogColumn::Title => Sort::Title,
            CatalogColumn::Year => Sort::Year,
            CatalogColumn::AddedDate => Sort::AddedDate,
            CatalogColumn::SpecStatus => Sort::SpecsStatus,
            CatalogColumn::TagStatus => Sort::TagsStatus,
        };
        view.descending = sort
            .and_then(|v| v.get("direction"))
            .and_then(Value::as_str)
            == Some("desc")
            && sort.and_then(|v| v.get("field")).and_then(Value::as_str)
                == Some(serde_json::to_value(&column)?.as_str().unwrap_or(""));
        *target = Some(columns);
    }
    state.presentation = Some(presentation);
    state.validate()?;
    Ok(state)
}
