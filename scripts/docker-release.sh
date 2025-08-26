#!/bin/bash
set -e

# Docker release script for Tenrankai
# Usage: ./scripts/docker-release.sh [version]
# Example: ./scripts/docker-release.sh v0.1.0

# Configuration
REGISTRY="ghcr.io"
NAMESPACE="theatrus"
IMAGE_NAME="tenrankai"
FULL_IMAGE="${REGISTRY}/${NAMESPACE}/${IMAGE_NAME}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Get version from argument or git tag
if [ -z "$1" ]; then
    VERSION=$(git describe --tags --abbrev=0 2>/dev/null || echo "dev")
    echo -e "${YELLOW}No version specified, using git tag: ${VERSION}${NC}"
else
    VERSION="$1"
fi

# Remove 'v' prefix if present for some tags
VERSION_NO_V="${VERSION#v}"

echo -e "${GREEN}Building Tenrankai Docker image version ${VERSION}${NC}"

echo -e "${YELLOW}Building Docker image...${NC}"

# Build the image
docker build -t "${IMAGE_NAME}:${VERSION}" .

if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ Built Docker image${NC}"
    
    # Tag for registry
    docker tag "${IMAGE_NAME}:${VERSION}" "${FULL_IMAGE}:${VERSION}"
    docker tag "${IMAGE_NAME}:${VERSION}" "${FULL_IMAGE}:${VERSION_NO_V}"
    
    # Tag as latest if this is not a pre-release
    if [[ ! "$VERSION" =~ -(alpha|beta|rc) ]]; then
        docker tag "${IMAGE_NAME}:${VERSION}" "${FULL_IMAGE}:latest"
    fi
    
    # Add major and minor version tags
    if [[ "$VERSION" =~ ^v?([0-9]+)\.([0-9]+)\.([0-9]+)$ ]]; then
        MAJOR="${BASH_REMATCH[1]}"
        MINOR="${BASH_REMATCH[2]}"
        docker tag "${IMAGE_NAME}:${VERSION}" "${FULL_IMAGE}:${MAJOR}"
        docker tag "${IMAGE_NAME}:${VERSION}" "${FULL_IMAGE}:${MAJOR}.${MINOR}"
    fi
else
    echo -e "${RED}✗ Failed to build Docker image${NC}"
    exit 1
fi

echo -e "${GREEN}Successfully built Docker image!${NC}"
echo ""
echo "Image built:"
docker images | grep "${IMAGE_NAME}" | grep "${VERSION}"

echo ""
echo -e "${YELLOW}To push images to registry, run:${NC}"
echo "docker login ${REGISTRY}"
echo "docker push ${FULL_IMAGE}:${VERSION}"
if [[ ! "$VERSION" =~ -(alpha|beta|rc) ]]; then
    echo "docker push ${FULL_IMAGE}:latest"
fi

echo ""
echo -e "${YELLOW}To test the image locally:${NC}"
echo "docker run --rm -p 8080:8080 -v \$(pwd)/config.toml:/app/config.toml:ro -v \$(pwd)/photos:/app/photos:ro ${IMAGE_NAME}:${VERSION}"