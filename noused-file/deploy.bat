git pull origin master
git fetch --all
git reset --hard origin/master

call install.bat
call build.bat

del ../H58991839.wgt /q/s
"c:\Program Files\7-Zip\7z.exe" a -tzip ../H58991839.wgt .\app\*
del ./version.json /q/s
del ../version.json /q/s
node version_json.js
copy version.json ../version.json
