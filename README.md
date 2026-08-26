<div align="center">

# IMDb Tech Manager

**English** | [简体中文](./README.zh-CN.md)

<p align="center">

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Stars](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

Technical specifications and metadata management for film & media libraries.

**🚧 Active Development · Source Code Coming Later**

</div>

---

## 🎬 About

**IMDb Tech Manager** is a project for managing technical information related to films and television productions.

It focuses on acquiring, structuring, normalizing, and applying **IMDb Technical Specifications**, turning relatively scattered production information into metadata that can be managed and presented inside personal media libraries.

The project currently focuses on areas such as:

* IMDb Technical Specifications
* NFO metadata management
* Technical tag generation
* Camera and lens information
* Film and digital capture formats
* Production and presentation formats
* Technical metadata normalization
* Media library presentation
* AI-assisted semantic processing
* Coding Agent-assisted development

Most media libraries already provide excellent information about titles, cast members, release years, resolution, codecs, and audio formats.

However, information such as **which cameras and lenses were used, how a production was captured, which film or digital formats were involved, and how the final work was mastered or presented** is rarely preserved in a structured and useful way.

IMDb Tech Manager aims to bring this information into the media library workflow.

---

## 🔄 Core Workflow

```text
IMDb Technical Specifications
        ↓
Data acquisition and structuring
        ↓
Technical specification normalization
        ↓
NFO metadata management
        ↓
Technical tag generation
        ↓
Technical information presentation in media libraries
```

The goal is not simply to preserve raw text from IMDb.

The project aims to turn technical specifications into structured information that can be:

* Managed
* Normalized
* Corrected
* Searched
* Tagged
* Presented
* Reused by other tools

---

## 📚 Technical Information

IMDb Technical Specifications contain a wide range of production information, including:

* 📷 Cameras
* 🔭 Lenses
* 🎞️ Film capture formats
* 💾 Digital capture formats
* 🎥 Cinematographic processes
* 🧪 Laboratory and post-production processes
* 🖼️ Aspect ratios
* 🔊 Sound mixes
* 📽️ Master and presentation formats
* 🎬 Printed film formats
* Other production-related Technical Specifications

IMDb Tech Manager builds parsing, normalization, metadata management, and presentation capabilities around this information.

---

## 🧩 Project Architecture

The project is conceptually divided into two major areas.

### 📦 Data Management

The data-management side is responsible for acquiring, processing, reviewing, and maintaining technical metadata.

Its responsibilities include:

* IMDb Technical Specifications acquisition
* Technical Specifications structuring
* NFO file management
* Writing Technical Specifications to NFO files
* Technical tag generation
* Technical specification normalization
* AI-assisted semantic processing
* Preview / Dry Run
* Manual correction
* Batch processing
* Metadata maintenance

### 🖥️ Media Library Presentation & Integration

The presentation and integration side is responsible for bringing processed technical information into media libraries.

Its responsibilities include:

* Technical specification cards
* Technical information presentation
* Metadata synchronization
* Web UI integration
* Compatibility handling for different media types
* Integration with media-library metadata workflows

### 🧭 Platform Independence

These two areas are **not permanently tied to a particular operating system**.

The architecture describes responsibilities rather than platforms.

Current implementations happen to focus on:

* A data-management application currently developed for **macOS**
* A media-library integration solution currently developed around **Emby**, with the current implementation running in a Windows environment

These are the platforms implemented today, not architectural limitations.

Future versions may support additional:

* Operating systems
* Media servers
* Deployment environments
* Client applications

---

## 🧠 Design Principles

### 1. Prefer deterministic rules

When a problem can be reliably solved using explicit rules, local deterministic logic is preferred.

Typical examples include:

* Known format mappings
* Tag normalization
* Data transformations
* NFO operations
* File processing
* Validation

AI should not be introduced merely for the sake of using AI.

### 2. Use AI for ambiguous semantics

AI is used where semantic interpretation is genuinely useful, such as:

* Irregular natural-language descriptions
* Ambiguous technical specifications
* Semantic decomposition
* Expressions that are difficult to cover with fixed rules

AI is treated as one capability of the system rather than the foundation of the entire project.

### 3. Keep AI providers replaceable

The model layer should remain configurable wherever practical.

Typical configuration includes:

```text
Provider
Base URL
API Key
Model
```

This makes it possible to switch model services according to:

* Capability
* Cost
* Availability
* Deployment environment
* Privacy requirements

### 4. Preview before modification

Operations with side effects should generally follow:

```text
Preview / Dry Run
        ↓
User confirmation
        ↓
Execution
```

This applies especially to:

* NFO modifications
* Technical tag writing
* Metadata changes
* Batch operations

### 5. Respect user-managed metadata

Automatically generated data should not silently destroy manually maintained metadata.

User-created corrections and tags should remain under user control.

### 6. Keep behavior observable

The project aims to make important background operations visible and verifiable.

Useful information may include:

* What was processed
* What was changed
* Which results came from local rules
* Which results came from AI
* API call count
* Token usage
* Cache hits
* Estimated cost
* Task results
* Errors and affected items

---

## 🤖 Agent-Friendly Development

IMDb Tech Manager is also exploring a development workflow designed for modern Coding Agents.

When the source code is publicly released, the repository plans to provide:

**`AGENTS.md`**

This file will act as a project-level guide for Coding Agents.

It is expected to describe:

* Project goals
* Architecture
* Repository structure
* Module responsibilities
* Module boundaries
* Development principles
* Design constraints
* Coding conventions
* Testing requirements
* Build procedures
* Release workflow
* Known issues
* Common pitfalls
* Required checks before completing a change

The intended workflow is:

```text
Fork / Clone
        ↓
Coding Agent reads AGENTS.md
        ↓
Understands architecture and project rules
        ↓
Analyzes affected modules
        ↓
Plans the change
        ↓
Modifies the code
        ↓
Runs tests
        ↓
Verifies the result
        ↓
Submits a Pull Request
```

The goal is to open more than just source code.

Where practical, the project also aims to share:

```text
Source Code
    +
Architecture Knowledge
    +
Development Rules
    +
Testing Workflows
    +
Agent Context
```

This should make the project easier to understand and modify, both for developers and for the Coding Agents assisting them.

---

## 🚧 Current Status

IMDb Tech Manager is currently under **active development**.

The public repository is currently being used for:

* 📖 Project documentation
* 🧭 Roadmap planning
* 💬 Feature discussions
* 🏗️ Architecture discussions
* 🧪 Development experiments
* 🐛 Issue tracking
* 🤝 Community feedback

### Source Code

**The source code has not yet been officially released.**

Before the source release, the project still needs to complete work such as:

* Code cleanup
* Sensitive-information review
* Git history review
* Repository restructuring
* Test improvements
* Development documentation
* `AGENTS.md`
* License selection
* Release workflow preparation

The source code will be added to this repository once it is ready for public release.

---

## 🗺️ Roadmap

* [ ] Improve public documentation
* [ ] Document the project architecture
* [ ] Complete source-release security review
* [ ] Review Git history before publication
* [ ] Prepare and publish `AGENTS.md`
* [ ] Establish a stable test workflow
* [ ] Open the core source code
* [ ] Establish a standard Release workflow
* [ ] Improve Technical Specifications normalization
* [ ] Expand supported technical-specification data
* [ ] Improve media-library presentation
* [ ] Explore additional operating-system support
* [ ] Explore additional media-server support
* [ ] Improve Coding Agent development workflows

The Roadmap will continue to evolve as the project develops.

---

## 💬 Discussions

Ideas, technical discussions, UI concepts, metadata rules, media-library integration ideas, and development workflows are welcome in:

[**GitHub Discussions →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

Discussions are also a good place for ideas that are not yet concrete enough to become formal Issues.

---

## 🐛 Issues

Confirmed bugs, reproducible problems, and clearly defined feature requests can be submitted through:

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

---

## 🤝 Contributing

The source code has not yet been publicly released.

At the current stage, contributions are mainly welcome through Discussions and Issues, including:

* Feature suggestions
* Technical approaches
* Metadata normalization ideas
* Camera / lens / production-format knowledge
* UI / UX feedback
* Bug reports
* Media-library integration ideas
* Agent workflow suggestions

More complete contribution guidelines will be provided when the source code is opened.

---

## 📄 License

A final open-source license has not yet been selected.

A clear `LICENSE` file describing the rules for using, modifying, and distributing the source code will be added before the public source release.

---

## ⚠️ Disclaimer

IMDb Tech Manager is an independently developed project.

It is **not officially affiliated with or endorsed by IMDb, Emby, or other third-party platforms**.

Third-party names, trademarks, data, and services belong to their respective owners.

Users are responsible for ensuring that their use of third-party data and services complies with applicable terms of service and legal requirements.

---

## 💡 Feedback

IMDb Tech Manager is still evolving.

If you have ideas about:

* IMDb technical data that should be supported
* Better normalization rules
* Cameras, lenses, or production formats
* Media-library presentation
* Additional media-server support
* Workflow automation
* Coding Agent integration

feel free to join the Discussions and share them.

Ideas are welcome even when the implementation is not yet obvious.
