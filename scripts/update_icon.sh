#!/bin/sh
# Regenerate the Windows icon from the png. Run this from the repository root
# after res/icons/icon.png changed and commit the result. Needs ImageMagick.
convert res/icons/icon.png -compress none -define icon:auto-resize=16,32,48,64,128,256 res/icons/icon.ico
