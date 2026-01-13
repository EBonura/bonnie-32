# Contributing to BONNIE-32

Thanks for your interest in contributing! This project is a passion project with a specific vision, but contributions are welcome.

## How to Contribute

### Reporting Bugs
- Open an issue with a clear description
- Include steps to reproduce
- Mention if it's native or web (WASM) build

### Suggesting Features
- Open an issue to discuss before implementing
- Keep in mind the PS1 aesthetic focus

### Pull Requests
1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Make your changes
4. Test both native and WASM builds:
   ```bash
   cargo run                                           # Native
   cargo build --target wasm32-unknown-unknown         # WASM
   ```
5. Submit a PR to `main`

### Code Style
- Follow existing patterns in the codebase
- Keep it simple - avoid over-engineering
- No unnecessary dependencies

## What's Needed
Check the README backlog for current priorities. Areas where help is especially welcome:
- Bug fixes
- Documentation
- Texture packs (with appropriate licensing)
- Testing on different platforms

## License
By contributing, you agree that your contributions will be licensed under the same license as the project.
