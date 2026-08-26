<div align="center">

# IMDb Tech Manager

[简体中文](./README.md) | **English**

<p align="center">

[![Release](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=release)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Downloads](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=downloads)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Stars](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

Technical specifications and metadata management for film & media libraries.

**🚧 Active Development · Source Code Coming Later**

---

## 🎬 About

**IMDb Tech Manager** is a project focused on managing technical information for film and television productions.

</p>

<img src="./docs/images/poster.jpg" alt="IMDb Tech Manager Poster" width="700">

</div>

</p>

The project focuses on acquiring, structuring, normalizing, and applying **IMDb Technical Specifications**, transforming relatively scattered production information into metadata that can be managed, searched, and presented inside personal media libraries.

Current areas of focus include:

* IMDb Technical Specifications
* NFO metadata management
* Technical tag generation
* Camera and lens information
* Film and digital capture formats
* Production and presentation formats
* Technical metadata normalization
* Media library technical information presentation
* AI-assisted semantic processing
* Coding Agent-assisted development

Most media libraries already provide excellent information about titles, cast members, release years, resolution, codecs, and audio formats.
However, information such as **which cameras and lenses were used, which film or digital capture formats were involved, what production processes were used, and how the final work was mastered and presented** is rarely preserved and presented in a complete, structured way.

IMDb Tech Manager aims to bring this information into the personal media-library workflow.

---
## 🖼️ Screenshots

### Data Management

<div align="center">

<img src="./docs/images/data-management.png" alt="IMDb Tech Manager Data Management" width="700">

</div>

</p>

The NFO data management side is used for IMDb Technical Specifications acquisition, technical specification organization, tag generation, and batch task management.

### NFO Management

<div align="center">

<img src="./docs/images/tag-management.png" alt="IMDb Tech Manager Tag Management" width="700">

</div>

</p>

Tag management is used to inspect, preview, and modify technical metadata in the media library, while allowing users to manually correct automatically generated content.

### AI Runtime Management

<div align="center">

<img src="./docs/images/ai-runtime-management.png" alt="IMDb Tech Manager AI Runtime Management" width="900">

</div>

</p>

The AI Runtime management interface is used to configure the model endpoint, API Base URL, Prompt Cache, inference parameters, system prompt, and other behaviors related to AI-based tag generation.

### Media Library Technical Specifications

<div align="center">

<img src="./docs/images/media-library-card.png" alt="IMDb Tech Manager Media Library Technical Specifications" width="900">

</div>

</p>

Processed technical specifications can be further used for technical information presentation inside media libraries.

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

The goal is not simply to preserve raw text from IMDb pages. Instead, the project aims to transform these technical specifications into structured information that can be:

* Managed
* Normalized
* Manually corrected
* Searched
* Tagged
* Presented
* Reused by other tools

---
## 📚 Technical Information

IMDb Technical Specifications contain a wide range of film and television production information, including:

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
IMDb Tech Manager builds parsing, normalization, metadata-management, and presentation capabilities around this information.

---
## 🧩 Project Architecture

Conceptually, IMDb Tech Manager is divided into two major areas.
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

The architecture describes responsibilities rather than platform limitations.

Current implementations focus on:

* A data-management application currently running on **macOS**
* A media-library presentation and integration solution currently developed around **Emby**
These are simply the implementations that currently exist and do not mean the project can only run on these platforms in the future.

Future versions may support:

* Additional operating systems
* Additional media servers
* Additional deployment environments
* Additional client applications

---
## 🧠 Design Principles

### 1. Prefer deterministic rules

When a problem can be reliably solved using explicit rules, deterministic local logic is preferred.

Examples include:

* Known format mappings
* Tag normalization
* Data transformations
* NFO operations
* File processing
* Validation

AI should not be introduced merely for the sake of using AI.
### 2. Use AI for ambiguous semantics

AI is better suited to:

* Irregular natural-language descriptions
* Ambiguous technical specifications
* Semantic decomposition
* Expressions that are difficult to cover using fixed rules

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

Different model services can therefore be selected based on capability, cost, availability, deployment environment, and privacy requirements.
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

Automatically generated data should not silently overwrite user-maintained metadata without a clear reason.

User-created additions, modifications, and corrections should remain under user control.
### 6. Keep behavior observable and verifiable

The project aims to make important background operations as transparent as practical.

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

When the source code is publicly released, the Repository plans to provide:

**`AGENTS.md`**

This file will act as an important entry point for Coding Agents to understand the project.

It is expected to include:
* Project goals
* Software architecture
* Repository structure
* Module responsibilities
* Module boundaries
* Development principles
* Design constraints that should not be broken
* Coding conventions
* Testing requirements
* Build procedures
* Release workflow
* Known issues
* Common development pitfalls
* Required checks before completing a code change

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

The project intends to make more than just its source code available.

Where practical, it also aims to share:
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

This should make the project easier for both developers and Coding Agents to understand, modify, and extend.

---
## 🚧 Current Status

IMDb Tech Manager is currently under **active development**.

The public Repository is currently being used for:

* 📖 Project documentation
* 🧭 Roadmap planning
* 💬 Feature discussions
* 🏗️ Architecture discussions
* 🧪 Development experiments
* 🐛 Issue tracking
* 🤝 Community feedback
### Source Code

**The source code has not yet been officially released.**

Before the source release, the project still needs to complete:

* Code cleanup
* Sensitive-information review
* Git history review
* Repository restructuring
* Test improvements
* Development documentation
* `AGENTS.md`
* License selection
* Release workflow preparation

The source code will be added to this Repository once preparations for the public release are complete.

---
## 🗺️ Roadmap
* [ ] Improve public documentation
* [ ] Document the project architecture
* [ ] Complete the pre-release security review
* [ ] Review Git history before publication
* [ ] Prepare and publish `AGENTS.md`
* [ ] Establish a stable testing workflow
* [ ] Open the core source code
* [ ] Establish a standard Release workflow
* [ ] Improve Technical Specifications normalization
* [ ] Expand supported technical-specification data
* [ ] Improve media-library technical information presentation
* [ ] Explore support for additional operating systems
* [ ] Explore support for additional media servers
* [ ] Improve Coding Agent development workflows
The Roadmap will continue to evolve as the project develops.

---
## 💬 Discussions

Feature ideas, technical approaches, UI design, data-normalization rules, media-library integration methods, and development workflows are all welcome here:

[**GitHub Discussions →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

If an idea is not yet concrete enough to become a formal Issue, Discussions are also a good place to explore it first.

---
## 🐛 Issues

Confirmed bugs, reproducible problems, and clearly defined feature requests can be submitted through:

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

---
## 🤝 Contributing

The project source code has not yet been officially released.

At the current stage, contributions are mainly welcome through Discussions and Issues, including:

* Feature suggestions
* Technical approaches
* Metadata-normalization ideas
* Camera / lens / production-format information
* UI / UX feedback
* Bug reports
* Media-library integration ideas
* Agent workflow suggestions
More complete contribution guidelines and Coding Agent development documentation will be provided when the source code is opened.

---
## 📄 License

A final open-source license has not yet been selected.

A clear `LICENSE` file describing the rules for using, modifying, and distributing the source code will be added before the public source release.

---
## ⚠️ Disclaimer

IMDb Tech Manager is an independently developed project.

It is **not officially affiliated with, authorized by, or endorsed by IMDb, Emby, or other third-party platforms**.

Third-party names, trademarks, data, and services belong to their respective owners.

Users are responsible for ensuring that their use of third-party data and services complies with applicable terms of service and legal requirements.

---
## 💡 Feedback

IMDb Tech Manager is still evolving.

If you have ideas about IMDb technical data, normalization rules, cameras and lenses, media-library presentation, support for other media servers, workflow automation, or Coding Agent integration, feel free to join the Discussions.

Ideas are welcome even when the implementation is not yet obvious.
