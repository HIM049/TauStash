# TauStash

A content-indexed file library with everything in tags.


## Why TauStash?

Folders force every file into a single place. A photo can't belong to several categories without duplicating it, and deep folder trees quickly become hard to browse.

TauStash takes a different approach. It indexes your files by content and organizes them with tags. Collections are built dynamically from tag filters, so one file can belong to as many collections as it fits.


## Features

- **Content Indexed** — Files are tracked by file fingerprint, not device or path.
- **Tag for Everything** — Status and category are all tags, so any combination can be filtered and reused.
- **Dynamic Collections** — Build views from tag filters, they update as your library changes.
- **Auto Scanning** — Watch folders such as Downloads or Pictures and import new files with preset tags.
- **Optional Store** — Copy or move archived files into a managed store.


## How it works

- **Files and replicas** — A file is identified by its content (hash). The same content found in several locations is one file with several replicas.
- **Sources** — Folders TauStash scans to discover and import files.
- **Tags** — The only way to organize. Archived, ignored and deleted are tags too.
- **Collections** — Set of filters saved to show files in need. Both folders and sidebar view is a collection.
- **Store** (optional) — A managed folder TauStash can copy or move files into.

> **Note:** Deleting a file in TauStash is a state for its content. If the same file shows up again in any source, it will be deleted automatically if the file already out of the recover time.


## Planned Features

- Mount collections as folders in system
- Sync between multiple devices
- ML Interface for tagging

## Others

Hope TauStash helps you! Feedback and bug reports are always welcome in [Issues](https://github.com/HIM049/TauStash/issues). 

If you would like to contribute, opening an issue before a PR is the best way to discuss changes.

Vibe-coded PRs are generally not accepted — please make sure you understand and have tested your changes.
