#!/bin/sh
# Regenerate the Windows icon from the png. Run this from the repository root
# after res/app_icons/icon.png changed and commit the result. Needs ImageMagick.
convert res/app_icons/icon.png -compress none -define icon:auto-resize=16,32,48,64,128,256 res/app_icons/icon.ico
