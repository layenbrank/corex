## 环境初始化

- powershell 执行

  - 初始化，将当前可执行文件，添加至环境变量

  ```powershell
  fluxor setup --env

  ```

- 检查是否在环境变量中

  ```powershell
  fluxor setup --check
  ```

## copy

| 参数   | 描述                     |
| ------ | ------------------------ |
| from   | 源路径                   |
| to     | 目标路径                 |
| ignore | 忽略项                   |
| empty  | 清空目标目录下的所有内容 |

- 示例

  ```powershell
  fluxor copy --from ./input --to ./output --ignore "example.js,*.git,node_modules"

  fluxor copy -f ./input -t ./output --ignore "example.js,*.git,node_modules"
  ```

## generate path

| 参数      | 描述                                                                   |
| --------- | ---------------------------------------------------------------------- |
| from      | 源路径                                                                 |
| to        | 目标文件路径                                                           |
| ignore    | 忽略项(`,`分割或者多次调用)                                            |
| transform | 转换规则                                                               |
| index     | 起始索引                                                               |
| separator | 路径分隔符                                                             |
| uppercase | 将某个规则转换为大写，可多次使用或用逗号分隔，同 ignore 一样可多次调用 |

- transform

  | 规则          |     |
  | ------------- | --- |
  | {{index}}     |     |
  | {{filename}}  |     |
  | {{extension}} |     |
  | {{path}}      |     |
  | {{fullpath}}  |     |

- 示例

  ```powershell
  fluxor generate path --from dist --to path.txt --ignore "example.js,*.git,node_modules" --index 1 --separator "\" --uppercase "extension" --transform '<include name="IDR_ITAB_{{extension}}_{{index}}" file="{{fullpath}}" type="BINDATA" />'
  ```
