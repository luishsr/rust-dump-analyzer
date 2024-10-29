# Rust Dump Analyzer

`rust-dump-analyzer` is a Rust-based tool for analyzing memory or binary dump files. This tool provides an interactive command-line UI with features for hex viewing, ASCII and pattern recognition, contextual information display, search, and navigation. It’s designed to assist in analyzing binary files, viewing patterns, and exploring ASCII strings.

## Features

- Hex dump view with address navigation
- ASCII string and known file pattern detection (e.g., PDF, JPEG, ZIP, PNG)
- Interactive UI with navigation and search
- Summary panel with key statistics (total entries, patterns detected, ASCII strings found)
- Contextual byte display for selected entries
- Search functionality for ASCII strings or hex patterns
- Jump-to-address feature for efficient navigation

## Installation

Ensure you have Rust installed. You can install Rust from [rust-lang.org](https://www.rust-lang.org/).

Clone the repository and build the project:

```bash
git clone https://github.com/luishsr/rust-dump-analyzer.git
cd rust-dump-analyzer
cargo build --release

## Usage

Run the analyzer on the test dump file:

```bash
cargo run --bin dump test_dump.bin

Key Commands
q: Quit the application
↑ / ↓: Navigate through entries
/: Open the search popup to search by ASCII string or hex pattern
g: Open the address input popup to jump to a specific address

## Contributing
Contributions are welcome! If you encounter bugs or have feature requests, feel free to open an issue or create a pull request.
