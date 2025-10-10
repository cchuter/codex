# Homebrew Setup for OsmiFlow

This document explains how to set up and use the Homebrew package for OsmiFlow.

## Prerequisites

1. **Create a Homebrew Tap Repository**
   - Create a new repository named `homebrew-osmiflow` in the `cchuter` GitHub account
   - This repository will host the Homebrew formula

2. **Create a GitHub Personal Access Token**
   - Go to GitHub Settings → Developer settings → Personal access tokens
   - Create a token with `repo` scope
   - Add it as a secret named `HOMEBREW_TAP_TOKEN` in your main repository settings

3. **Initialize the Tap Repository**
   ```bash
   git clone https://github.com/cchuter/homebrew-osmiflow
   cd homebrew-osmiflow
   mkdir Formula
   git add Formula
   git commit -m "Initialize tap repository"
   git push
   ```

## How It Works

### 1. Release Process (`homebrew-release.yml`)

When you create a new release (tag with `v*` prefix):

1. **Build Phase**: Builds binaries for:
   - macOS ARM64 (M1/M2)
   - macOS x86_64 (Intel)
   - Linux x86_64

2. **Release Phase**: 
   - Creates GitHub release with all binaries
   - Calculates SHA256 checksums

3. **Formula Update Phase**:
   - Automatically updates the formula in `homebrew-osmiflow` repository
   - Updates version and checksums

### 2. Bottle Building (`homebrew-bottles.yml`)

After the release workflow:

1. **Bottle Creation**: Builds optimized bottles for:
   - macOS Ventura
   - macOS Sonoma
   - Linux x86_64

2. **Upload**: Attaches bottles to the GitHub release

3. **Formula Update**: Adds bottle DSL to the formula for faster installation

## Usage

### Creating a Release

1. **Tag and Push**:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```

2. **Or Manual Trigger**:
   - Go to Actions → Homebrew Release
   - Click "Run workflow"
   - Enter version number

### Installing OsmiFlow

Once the release workflow completes:

```bash
# Add the tap (one-time)
brew tap cchuter/osmiflow

# Install osmiflow
brew install osmiflow

# Run osmiflow
osmiflow --help
```

### Updating OsmiFlow

```bash
brew update
brew upgrade osmiflow
```

## File Structure

```
.github/
├── workflows/
│   ├── build.yml                 # Basic build workflow
│   ├── homebrew-release.yml      # Release and formula update
│   └── homebrew-bottles.yml      # Bottle building
├── homebrew-formula-template.rb  # Formula template
└── HOMEBREW_SETUP.md             # This file
```

## Troubleshooting

### Formula Not Found
- Ensure the tap repository exists: `https://github.com/cchuter/homebrew-osmiflow`
- Check that the Formula directory contains `osmiflow.rb`

### Installation Fails
- Check that the release assets are properly uploaded
- Verify SHA256 checksums match

### Bottle Installation Slow
- Wait for the bottle workflow to complete after release
- Bottles significantly speed up installation

## Manual Formula Update

If automatic update fails, manually update the formula:

1. Clone the tap repository
2. Update `Formula/osmiflow.rb` with new version and checksums
3. Commit and push

## Notes

- The binary is renamed from `codex` to `osmiflow` during packaging
- Supports macOS (Intel & ARM) and Linux (x86_64)
- Linux ARM support can be added by modifying the workflows