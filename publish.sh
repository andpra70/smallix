#!/usr/bin/env bash
set -euo pipefail

git add .
git commit -m "Publish new version"
git push origin main
