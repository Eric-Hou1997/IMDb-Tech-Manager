use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(rename = "AiRuntime")]
pub struct State {
    pub paused: bool,
    pub reason_kind: String,
    pub reason: String,
    pub paused_at: String,
    pub last_success_at: String,
    pub last_error_at: String,
    pub updated_at: String,
}
impl State {
    pub fn error(&self) -> Option<AppError> {
        self.paused.then(|| {
            AppError::new(
                match self.reason_kind.as_str() {
                    "budget" => "budget-exhausted",
                    "" => "paused",
                    kind => kind,
                },
                if self.reason.is_empty() {
                    "AI runtime is paused"
                } else {
                    &self.reason
                },
            )
        })
    }
    pub fn clear(&mut self) {
        self.paused = false;
        self.reason_kind.clear();
        self.reason.clear();
        self.paused_at.clear();
        self.updated_at = job::now();
    }
    pub fn import(value: Value) -> (Self, Option<String>) {
        match serde_json::from_value(value){
            Ok(state)=>(state,None),
            Err(_)=>(Self{paused:true,reason_kind:"legacy-runtime-unverified".into(),reason:"Historical AI runtime state could not be verified; explicitly resume after reviewing the archived source".into(),..Default::default()},Some("Unverified historical runtime is imported as paused; no request resumes automatically".into()))
        }
    }
}
