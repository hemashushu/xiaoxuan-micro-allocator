# XiaoYu 内核 heap 动态分配器

专为 32 位微控制器（MCU）而设计的高效内核 _heap_ 动态分配器。

特点：

- 为内存资源非常有限（最小支持 4KiB）的 MCU 而优化，分配器本身占用资源低；
- 分配和释放的速度快；
- 降低内部碎片和外部碎片的产生；

动态分配器默认为 _小容量_ 模式，最大支持 128 MiB 的内存。分配器可配置为 _大容量_ 模式，最大支持 4 GiB 内存，可应用在 32 位微处理器（MPU）环境中。如果需要用在 64 位的环境，请使用另一个分配器 [XiaoXuan Allocator](https://www.github.com/hemashushu/xiaoxuan-allocator) 。

<!-- @import "[TOC]" {cmd="toc" depthFrom=2 depthTo=5 orderedList=false} -->

<!-- code_chunk_output -->

- [原理](#-原理)
  - [内存的结构](#-内存的结构)
  - [_Heap_ 的结构](#-_heap_-的结构)
    - [_Header_ 的结构](#-_header_-的结构)
    - [_Data area_ 的结构](#-_data-area_-的结构)
      - [Data area 示例](#-data-area-示例)
      - [_Node_ 的结构](#-_node_-的结构)
      - [_Block_ 的结构](#-_block_-的结构)
      - [标记位](#-标记位)
  - [_Free memory link list_ 的工作原理](#-_free-memory-link-list_-的工作原理)
    - [Node 的插入及合并](#-node-的插入及合并)
  - [固定大小内存分配器](#-固定大小内存分配器)
  - [数据页的结构](#-数据页的结构)
    - [索引页和数据页示例](#-索引页和数据页示例)
    - [_空闲项_ 的结构](#-_空闲项_-的结构)
    - [_已分配项_ 的结构](#-_已分配项_-的结构)
    - [固定大小分配器的工作原理](#-固定大小分配器的工作原理)

<!-- /code_chunk_output -->

## 原理

本分配器由 _可变大小_ 以及 _固定大小_ 两个分配程序组成。其中 _可变大小_ 内存片段分配使用一个 _显式的剩余内存双向链表_ 实现，而 _固定大小_ 使用一个 _索引页_ 和多个 _单向链表页_ 实现。

### 内存的结构

```
  low addr                                         high addr
|------------------------------------------------------------|
| .text .rodata .data .bss | heap ->   |        |   <- stack |
|                          ^           ^        ^            |
|                       heap start    brk       sp           |
|------------------------------------------------------------|
```

分配器负责分配上图当中的 heap 区域的内存，也就是从 `heap start` 到 `brk` 之间的区域。

### _Heap_ 的结构

```
| header | data area |
```

#### _Header_ 的结构

```
| magic number | data area pos | index page pos | first node pos | brk |
```

- `magic number`：[uint32, uint32]，用于表示 XiaoYu Allocator 的幻数，值为 HEX `78 79 61 6c 6c 6f 63 01`，对应字符串 `xyalloc`，最后的 `0x01` 表示版本号。`magic number` 用于标识 _heap_ 是否已经初始化；
- `data area pos`：uint32，_data area_ 的开始地址；
- `index page pos`：uint32，_index page_ 的开始地址；
- `first node pos`：uint32，第一个 _node_ 的地址；
- `brk`：uint32，`heap` 的结束位置。

#### _Data area_ 的结构

_Data area_ 由 _零个或多个已分配的内存片段_ 以及 _一个或多个未分配的内存片段_ 组成。

> **已分配** 是指该部分内存片段已分配给程序所使用。对于已分配内存，在程序未释放之前，分配器不得删除、修改、移动该部分内存片段。

为了方便描述，内存片段根据不同的分配状态使用不同的名称来称呼：

- 已分配的内存片段称为 _block_ （_allocated memory block_）；
- 未分配的内存片段称为 _node_ （_doubly linked list node_）。

在分配器初始化之后，_data area_ 只存在一个 _node_，这个 _node_ 占据了整个 _data area_。

随着时间的推移，许多 _block_ 将会被创建，同时也有许多 _block_ 被释放，被释放的 _block_ 会重新转换为 _node_。这时 _data area_ 会存在着许多个被 _block_ 间隔开来的 _node_。

分配器会将所有 _node_ 串联起来，形成一个显式的双向链表，称为 _free memory linked list_。分配器使用该链表管理空闲的内存。

该链表是一个 _有序链表_，所有 _node_ 按照从 `低端地址` 到 `高端地址` 的顺序排列并连接。当一个 _block_ 被释放后，会根据它所在的位置插入到链表的相应位置。如果相邻的位置刚好也是 _node_，则会跟它们合并。

> _free memory linked list_ 总会有一个 _node_ 位于链表的末尾。

分配器由 _可变大小_ 和 _固定大小_ 两个程序组成，_data area_ 由 _可变大小_ 程序管理，即上面所说的 _free memory linked list_，_node_ 以及 _block_ 均由 _可变大小_ 程序维护。

当程序请求或者释放一个容量大于 128 bytes 的内存片段时，则由 _可变大小_ 程序负责。而小于等于 128 bytes 的内存片段则由 _固定大小_ 程序负责。

##### Data area 示例

下面的图演示了一个包含有 3 个 _block_ 和 2 个 _node_ 的 _data area_：

```
| ==== | ====== | //// | ===== | //////// |
```

图例：

- `==` 表示 _block_，即已分配的内存片段；
- `//` 表示 _node_，即未分配的内存片段。

##### _Node_ 的结构

```
| node size | next node pos | free memory | previous node pos | node size |
```

- `previous node pos` 和 `next node pos`：uint32，分别是前一个和后一个 _node_ 的位置。
- `node size`：uint32，是当前 _node_ 的总长度（即 **包括** `node offset` 和 `node size` 等字段在内的长度）。

当一个 _node_ 是链表的第一个 _node_ 时，`previous node pos` 的值为 0，当 _node_ 是链表的最后一个 _node_ 时，`next node pos` 的值为 0。显然，当整个链表只包含一个 _node_ 时，它的 `previous node pos` 和 `next node pos` 的值都是 0。

##### _Block_ 的结构

```
| block size | data | block size |
```

##### 标记位

分配器要求内存片段的大小必须是 4 的倍数（同时也是为了实现内存的 4 bytes 对齐），所以 `node size` 和 `block size` 数值的低 2 bits 的值总是 0。分配器将这 2 bits 用作为标记位，其中最低位是 `type flag`，第 2 低位是 `status flag`，其数值及含义如下：

- `status flag` == 0：表示 _node_；
- `status flag` == 1：表示 _block_；
- `type flag` == 0：表示可变大小内存片段；
- `type flag` == 1：表示固定大小内存片段。

### _Free memory link list_ 的工作过程

当分配器初始化之后，_free memory link list_ 只包含一个 _node_，该 _node_ 占据了整个 _data area_。

分配 _可变大小_ 的内存片段：

1. 分配器从链表的第一个 _node_ 开始遍历，如果找到的 _node_ 容量不足时，则继续往后找；

2. 当找到一个容量等于请求的大小的 _node_ 时，_node_ 会被转换为 _block_ （_allocated block_）并更新链表（详细见下一节）；

3. 当找到一个容量大于请求的大小的 _node_ 时，分配器会从 _node_ 的左边（低地址端）分割一部分出来并成为一个 _block_ ，而 _node_ 会缩小至剩下的部分。

4. 当分配器遍历到最后一个 _node_ 且容量不足以分配时，分配器会尝试增加 _heap_ 的空间，即向高地址端移动 `brk`。如果扩充成功，则 _node_ 先会增大容量，然后再进行分割。如果扩充失败，则分配失败。

释放 _可变大小_ 的内存片段（即 _block_）：

1. 当一个 _block_ 被释放时，分配器会把该 _block_ 转换为 _node_，并根据 _block_ 的位置插入到 _free memory linked list_ 相应的位置；

2. 新的 _node_ 插入到 _free memory linked list_ 时，如果相邻的位置也是 _node_，则新的 _node_ 将会跟相邻的 _node_ 合并（详细见下一节）。

> 显然在链表里，不存在相邻的两个 _node_，因为它们会被合并。

#### Node 的插入及合并

当一个由 _block_ 转换过来的新 _node_ 插入到 _free memory linked list_ 时，会有以下这几种情况：

1. 前后均为 _block_

```
| ====== | ===== | ====== |
             |
             | block
             v
| ====== | ///// | ====== |
```

这种情况不需要合并 _node_，只需把 _block_ 转换为 _node_ 即可。具体需要更新的字段有：

- 位于当前 _block_ 之前的第一个 _node_ 的 `next node pos` 字段，更新它的值为当前 _block_ 的位置。
  注：如果当前 _block_ 的位置比 `first node pos` 小，则说明不存在位于它之前的 _node_，这时需要更新 `first node pos` 字段，让它的值为当前 _block_ 的位置。也就是说，当前 _block_ 将会成为第一个 _node_；
- 位于当前 _block_ 之后的第一个 _node_ 的 `previous node pos` 字段，更新它的值为当前 _block_ 的位置；
- 当前 _block_ 的 `block size` 字段转换为 `node size` 字段，即更新 `status flag` 为 0；
- 当前 _block_ 添加 `next node pos` 和 `previous node pos` 字段，分别指向下一个和前一个 _node_ 的位置。注：如果当前 _block_ 将要成为第一个 _node_，则 `previous node pos` 的值应该为 0。

> 为了寻找当前 _block_ 之前及之后的第一个 _node_，分配器采用遍历 _free memory linked list_ 的方法，即从第一个 _node_ 开始遍历，找到第一个位置比当前 _block_ 大的 _node_，该 _node_ 则为当前 _block_ 的后一个 _node_。读取该 _node_ 的 `previous node pos`，则找到当前 _block_ 的前一个 _node_。

2. 前一个是 _node_

```
| ////// | ===== | ====== |
             |
             | block
             v
| ////////////// | ====== |
```

这种情况需要合并前一个 _node_。具体需要更新的有：

- 前一个 _node_ 的第一个 `node size` 字段，更新它的值为它们两者的总长度；
- 当前 _block_ 的第二个 `block size` 字段转换为 `node size` 字段，把它的值更新为它们两者的总长度；
- 当前 _block_ 添加 `previous node pos` 字段，其值为前一个 _node_ 的位置。

3. 后一个是 _node_

```
| ====== | ===== | ////// |
             |
             | block
             v
| ====== | ////////////// |
```

这种情况需要合并后一个 _node_。具体需要更新的有：

- 后一个 _node_ 的第二个 `node size` 字段，更新它的值为它们两者的总长度；
- 当前 _block_ 的第一个 `block size` 字段转换为 `node size` 字段，把它的值更新为它们两者的总长度；
- 当前 _block_ 添加 `next node pos` 字段，其值为前一个 _node_ 的位置。

4. 前后都是 _node_

```
| ////// | ===== | ////// |
             |
             | block
             v
| /////////////////////// |
```

这种情况需要合并前后两个 _node_。具体需要更新的有：

- 前一个 _node_ 的第一个 `node size` 字段，更新它的值为它们三者的总长度；
- 后一个 _node_ 的第二个 `node size` 字段，更新它的值为它们三者的总长度。

### 固定大小内存分配器

_固定大小分配器_（_fix size allocator_）是 XiaoYu Allocator 的一个子系统，它运行于 _可变大小分配器_ 的基础之上。用于快速分配具有固定大小且容量较小（小于等于 128 bytes）的内存。

_固定大小分配器_ 将内存片段按照长度每 4 bytes 分为一 _类_（_class_），比如 `1 ~ 4 bytes`（包括 1 和 4）作为第 1 类，`5 ~ 8 bytes`（包括 5 和 8）作为第 2 类，`9 ~ 12 bytes`（包括 9 和 12）作为第 3 类，如此类推。128 bytes 以内的长度共被分为 `128 / 4 = 32` 类。

固定大小分配器由 1 个 _索引页_（_index page_） 和 32 个 _数据页_（_data page_） 组成。_数据页_ 里面是一个单向链表（下面章节会讲解）。索引页包含有 32 个 uint32 整数（称为 _head item pos_），每一个整数指向一个单向链表的表头。

> _索引页_ 和 _数据页_ 均被包装在 _block_ 里（所以 _固定大小分配器_ 依赖于 _可变大小分配器_）。

### 数据页的结构

一个数据页里存储着多个长度相同的内存片段，内存片段被称为 _内存项_（_memory item_）。根据分配状态的不同 _内存项_ 也有不同的名称：

- 空闲的 _内存项_ 称为 _空闲项_ （_free item_）
- 已分配的称为 _已分配项_ (_allocated item_)

所有同一 _类_ 的 _空闲项_ 将会被连接起来形成一个单向链表，称为 _空闲项链表_ （_free item linked list_）。

当一个数据页的所有 _空闲项_ 都被分配之后，分配器会新建一个同类的 _数据页_，并且将新的 _数据页_ 连接到 _空闲项列表_ 的末尾。

#### 索引页和数据页示例

```
 index page          data page
|-------------|     |----------------------------------|
| idx 0 class -------> item -> item -> item -> item -> |
| idx 1 class ---\  |  item -> item -> item -> item -> |
| ...         |  |  |  item -> item -> item -> item    |
|             |  |  |----------------------------------|
|             |  |
|             |  |  |----------------------------------|
|             |  \---> item -> item -> item -> item -> |
|             |     |  item -> item -> item -> item ------\
|-------------|     |----------------------------------|  |
                                                          |
                 /----------------------------------------/
                 |
                 |  |----------------------------------|
                 \---> item -> item -> item -> item -> |
                    |  item -> item -> item -> item    |
                    |----------------------------------|
```

#### _空闲项_ 的结构

```
| next free item pos | class id | flags | free memory |
```

- `next free item pos`：25 bits，下一个 _空闲项_ 的开始位置；
- `class id`：5 bits，当前项的 _类_，值的范围是从 0 到 31；
- `flags`：2 bits，跟 _block_ 和 _node_ 一样，_item_ 的低端 2 bits 也是标记位，其中最低端位的值为 1，表示固定大小的内存片段。第 2 低端位的值忽略。

注意：`next free item pos`，`class id` 和 `flags` 是同一个 uint32 整数的不同部分。由于 `next free item pos` 只有 25 位，加上 4 bytes 对齐的 2 位，因此固定大小分配器的寻址范围是 `2^(25+2)` = 128 MiB。

当分配器被配置为 _大容量_ 模式时，`next free item pos` 将会单独作为一个 uint32，而 `class id` 和 `flags` 则属于另一个 uint32。此时寻址范围是 uint32 的最大值，即 4GiB。

#### _已分配项_ 的结构

```
| reserved | class id | flags | data |
```

_已分配项_ 是由 _空闲项_ 转换过来的，前 3 个字段的值保持不变，仅 `free memory` 被替换成实际的数据。

注意：原先 `next free item pos` 字段的值此时不再具有任何意义（因为已分配项已经不在 _free item linked list_ 里面），不过为了减少读写操作，该字段的原由的值将保持不变。

#### 固定大小分配器的工作过程

分配一个 _固定大小_ 的内存片段（即容量小于等于 128 bytes 的内存片段）：

1. 根据内存片段的大小计算类别 id，公式是 `(n - 1) / 4`，比如 8 bytes 的 `class id` 是 `(8 - 1) / 4 = 1`；

2. 在 _索引页_ 找到索引值为 1 的 `head item pos`；

   如果 `head item pos` 的值为 0，说明该 _class_ 对应的 _数据页_ 尚未创建。此时向 _可变大小分配器_ 申请一个刚好大于 128 byte 的 _block_，然后在里面初始化一个由多个 _free item_ 组成的 _free item linked list_，每一个 _free item_ 均指向下一个 _free item_，最后一个 _free item_ 的 `next item pos` 值为 0。最后更新 `head item pos`，让它的值为第一个 `free item` 的位置。

   比如长度为 8 bytes 的 _class_ 的 _item_ 长度为 `8 + 4 = 12 bytes`，于是申请一个长度为 132 bytes，即 `(int(128 / 12) + 1) * 12 = 11 * 12 = 132 bytes`）的 _block_，并在里面创建包含 22 个 _free item_ 的链表。

3. 将 `head item pos` 指向的 _free item_ 转换为 _allocated item_，然后更新 `head item pos`，让它的值为原链表的第二个 _free item_ 的位置；

释放一个 _固定大小_ 的内存片段（即 _allocated item_）：

1. 读取被释放的 _item_ 的 `class id` 字段，然后获取该 _class_ 的 `head item pos` 值；

2. 更新被释放的 _item_ 的 `next free item pos` 字段，让它的值等于 `head item pos`；

3. 更新 `head item pos` 字段，让它的值等于被释放的 _item_ 的位置，这样一来，被释放的 _item_ 又重新被插入到 _空闲项链表_ （_free item linked list_）。

> 虽然 _空闲项链表_ 是一种链表结构，但工作过程实际上更接近数据结构当中的 _栈_（_statck_）概念，该栈里是一堆 _free item_。当需要分配内存片段时，从该 _栈_ 弹出一项 _free item_；当 _已分配项_ 被释放时，该内存片段会压入 _栈_ 并称为栈顶项。可见 _固定大小分配器_ 比起 _可变大小分配器_，它节省了分割与合并链表项、搜索前后 _node_、遍历 _node_ 以找到合适大小的 _node_ 等等操作，因此 _固定大小分配器_ 的效率非常高，能胜任频繁的分配和释放任务。

> 当内存段落被释放时，怎样才能知道它是属于 _可变大小_ 的 _block_，还是 _固定大小_ 的 _allocated item_ 呢？还记得 _标记位_ 吗，其中的最低位 `type flag` 可以用于区分该内存片段的类型。

