# NPM Setup for OsmiFlow

This document explains how to set up and use the NPM package for OsmiFlow.

## Prerequisites

1. **NPM Account & Organization**
   - Create an NPM account at https://www.npmjs.com
   - Create the `@osmi` organization (or use your personal scope)
   - Generate an NPM access token with publish permissions

2. **GitHub Secret**
   - Add the NPM token as `NPM_TOKEN` in your repository secrets
   - Go to Settings → Secrets and variables → Actions → New repository secret

## How It Works

### Release Workflow (`npm-release.yml`)

The workflow automatically publishes to NPM when:
1. You push a tag starting with `v` (e.g., `v1.0.0`)
2. You manually trigger the workflow with a version

### Build Process

1. **Multi-Platform Build**: Builds native binaries for:
   - macOS ARM64 (M1/M2)
   - macOS x64 (Intel)
   - Linux x64
   - Windows x64

2. **Package Creation**: Creates an NPM package with:
   - Platform-specific binaries
   - Automatic binary selection during installation
   - Global CLI command registration

3. **Publishing**: Publishes to NPM with proper versioning and tags

## Usage

### Publishing a Release

**Option 1: Tag-based Release**
```bash
git tag v1.0.0
git push origin v1.0.0
```

**Option 2: Manual Workflow Dispatch**
1. Go to Actions → NPM Release
2. Click "Run workflow"
3. Enter version and npm tag (latest, beta, next)

### Installing OsmiFlow

Once published, users can install globally:

```bash
# Install globally
npm install -g @osmi/osmiflow

# Or with yarn
yarn global add @osmi/osmiflow

# Or with pnpm
pnpm add -g @osmi/osmiflow
```

### Using OsmiFlow

```bash
# Run osmiflow
osmiflow --help

# Start interactive session
osmiflow

# Ask specific question
osmiflow "explain this code"
```

## Local Development

### Building Locally

```bash
cd npm-package
npm run prepublishOnly  # Builds the binary
```

### Testing Installation

```bash
cd npm-package
npm link  # Creates global symlink
osmiflow --help  # Test the command
npm unlink  # Remove symlink
```

### Manual Publishing

```bash
cd npm-package
npm version 1.0.0
npm publish --access public
```

## File Structure

```
npm-package/
├── package.json        # NPM package configuration
├── index.js           # Entry point wrapper
├── postinstall.js     # Post-install script for binary setup
├── build.js          # Local build script
├── README.md         # Package documentation
├── .npmignore        # Files to exclude from package
└── bin/              # Binary files (created during build)
    ├── osmiflow-darwin-arm64
    ├── osmiflow-darwin-x64
    ├── osmiflow-linux-x64
    └── osmiflow-win32-x64.exe
```

## How It Works Internally

1. **Binary Selection**: The `postinstall.js` script:
   - Detects the user's platform and architecture
   - Creates a symlink to the appropriate binary
   - Falls back with error message for unsupported platforms

2. **Execution**: The `index.js` script:
   - Acts as a Node.js wrapper
   - Spawns the actual Rust binary
   - Passes through all arguments and stdio

## Versioning Strategy

- **latest**: Stable releases (default)
- **beta**: Beta versions for testing
- **next**: Upcoming features

Example:
```bash
# Publish as beta
npm publish --tag beta

# Users install beta with
npm install -g @osmi/osmiflow@beta
```

## Troubleshooting

### "Unsupported platform" Error
- Check the supported platforms list
- Open an issue for new platform support

### Permission Errors
- Use `sudo` for global installation on Unix systems
- Or configure npm to use a different directory

### Binary Not Found
- Ensure postinstall script ran successfully
- Check that the correct binary exists in `node_modules/@osmi/osmiflow/bin/`

### Publishing Fails
- Verify NPM_TOKEN is set correctly
- Ensure you have publish access to the @osmi scope
- Check that the version doesn't already exist

## Adding New Platforms

To add support for new platforms:

1. Update the build matrix in `npm-release.yml`
2. Add platform mapping in `postinstall.js`
3. Update supported platforms in documentation

## Security Notes

- Binaries are built in GitHub Actions for transparency
- Each binary is platform-specific to reduce package size
- Post-install script validates platform compatibility