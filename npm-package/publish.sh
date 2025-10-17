#!/bin/bash
set -e

echo "🚀 OsmiFlow NPM Package Publisher"
echo "================================="
echo ""

# Check if logged into npm
if ! npm whoami &>/dev/null; then
    echo "❌ Not logged into npm. Please run: npm login"
    echo "   Make sure you have access to the @osmiai organization"
    exit 1
fi

NPM_USER=$(npm whoami)
echo "✅ Logged in as: $NPM_USER"

# Check package name and version
PACKAGE_NAME=$(node -p "require('./package.json').name")
PACKAGE_VERSION=$(node -p "require('./package.json').version")

echo "📦 Package: $PACKAGE_NAME@$PACKAGE_VERSION"
echo ""

# Check if this is a private package or if we need to set access
if [[ "$1" == "--private" ]]; then
    echo "🔒 Publishing as private package..."
    ACCESS_FLAG="--access restricted"
elif [[ "$1" == "--public" ]]; then
    echo "🌍 Publishing as public package..."
    ACCESS_FLAG="--access public"
else
    echo "Please specify package access:"
    echo "  ./publish.sh --private   # For private @osmiai packages"
    echo "  ./publish.sh --public    # For public packages"
    exit 1
fi

echo ""
echo "This will publish $PACKAGE_NAME@$PACKAGE_VERSION to npm"
read -p "Continue? (y/n) " -n 1 -r
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cancelled."
    exit 1
fi

# Run the build process
echo ""
echo "🔨 Building package..."
if [ -f "build.js" ]; then
    node build.js
else
    echo "⚠️  No build.js found, skipping build step"
fi

# Create tarball for preview
echo ""
echo "📦 Creating package preview..."
npm pack --dry-run

echo ""
echo "🚀 Publishing to npm..."
npm publish $ACCESS_FLAG

echo ""
echo "✅ Successfully published $PACKAGE_NAME@$PACKAGE_VERSION!"
echo ""
echo "To install this package:"
echo "  npm install $PACKAGE_NAME"