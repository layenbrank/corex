var fs = require('fs')

function formatdate(d1) {
  var y = d1.getFullYear()
  var m = d1.getMonth() + 1
  var d = d1.getDate()
  if (m.toString().length == 1) {
    m = '0' + m
  }

  if (d.toString().length == 1) {
    d = '0' + d
  }
  var d_f = y.toString() + m + d
  return d_f
}

// 异步打开文件
console.log('准备文件！')
fs.open('./src/assets/js/version.js', 'w+', function (err, fd) {
  if (err) {
    return console.error(err)
  }
  console.log('打开成功！')

  var strs = []
  // 对象数据加密方法
  strs.push('export function version(){   ')
  strs.push('    return ' + formatdate(new Date()) + ';')
  strs.push(' }')

  fs.writeFile(fd, strs.join('\r\n'), function (e) {
    if (e) {
      return console.error(e)
    }
    console.log('写入成功！')
  })
})
