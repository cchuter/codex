#!/bin/bash
# Publish to GitHub Packages (free for private packages)

echo "Publishing to GitHub Packages..."
echo ""
echo "First, make sure you have authenticated with GitHub Packages:"
echo "  npm login --registry=https://npm.pkg.github.com --scope=@osmiai"
echo ""
echo "Then update package.json repository field to point to your GitHub repo"
echo ""

# Update registry for @osmiai scope
npm config set @osmiai:registry https://npm.pkg.github.com

# Publish the package
npm publish --ignore-scripts

echo "Package published to GitHub Packages!"
echo ""
echo "To install from GitHub Packages:"
echo "  npm config set @osmiai:registry https://npm.pkg.github.com"
echo "  npm install @osmiai/osmiflow"