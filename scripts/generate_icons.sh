cd $(git rev-parse --show-toplevel)

npm install fantasticon
npx fantasticon ./res/ui_icons/ -o ./res/fonts/ -t ttf -g css --name ui_icons
