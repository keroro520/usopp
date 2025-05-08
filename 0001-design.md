# Ussop: Solana多RPC节点性能基准测试

## 背景与目标

在Solana网络中，不同的RPC节点可能在交易处理速度和确认延迟方面存在差异。为了比较多个Solana RPC节点的性能，我们设计一个用Rust编写的基准测试程序。该程序将从同一个发送方生成N笔简单的SOL转账交易，并**并发**地将每笔交易发送到所有提供的RPC节点，同时通过WebSocket订阅各节点的交易确认状态。我们将测量每个节点在交易处理各阶段的耗时，并最终输出结构化的JSON结果用于分析。

## 功能需求

根据要求，程序需实现以下功能：

* **参数配置**：支持从配置文件或命令行获取必要参数，包括发送方密钥（Keypair文件路径）、接收方地址（Pubkey）、转账金额（以lamports为单位）、要发送的交易数量N，以及多个RPC节点的地址列表（每个节点包含HTTP RPC地址和对应的WebSocket地址）。

* **交易生成与发送**：程序根据提供的`from`地址、`to`地址和`value`（lamports）生成N笔SOL转账交易。对于每笔交易：

  1. 获取当前最新的区块哈希（latest blockhash）作为交易的 **recent blockhash**。
  2. 构建转账交易（使用System Program的转账指令）并使用发送方密钥对交易进行签名。
  3. 将签名后的交易**并发**发送到提供的所有RPC节点的HTTP接口（JSON-RPC接口）。也就是说，对每一笔交易，所有RPC节点同时接收该交易提交，以比较不同节点处理同一笔交易的速度。

* **确认订阅与监听**：针对每笔交易，在通过HTTP提交后，程序通过WebSocket在每个RPC节点上订阅该交易签名的确认状态。当某节点的WebSocket通知该交易达到**finalized**确认状态时，记录确认时间。Solana的WebSocket订阅接口`signatureSubscribe`支持对指定签名的确认通知订阅，并在交易达到指定确认级别（如finalized）时触发通知。订阅为一次性，当收到通知后服务器会自动取消订阅。

* **性能数据记录**：程序需要测量并记录每个阶段的耗时，包括：

  * **构建与签名时间**：获取区块哈希并构建、签名交易所耗时间。
  * **发送耗时**：每个RPC节点从发送交易请求开始到收到提交响应所耗时间。
  * **确认耗时**：每个RPC节点从交易发送（或提交响应）到收到最终确认通知所耗时间，即交易在该节点达到finalized的延迟。

* **结果汇总输出**：程序收集所有节点的测试结果，将其输出到一个JSON文件中。JSON结果应结构化且便于分析，包括每条交易在每个节点上的详细数据：节点URL、交易签名、各阶段耗时（构建、发送、确认），以及最终交易状态（确认成功或失败）。通过分析该JSON，可以比较各RPC节点在交易处理过程中的性能表现。

## 设计与实现方案

为满足上述需求，我们采用Rust并结合Solana官方SDK和异步编程框架进行实现。下面将详细阐述各个模块的设计。

### 配置读取

程序首先读取运行参数，可以使用Rust的命令行解析库（如**clap**）或配置文件读取模块。需获取以下配置：

* **发送方密钥对**：通过提供的文件路径加载Solana钱包的Keypair。例如使用`solana_sdk::signature::read_keypair_file`函数读取JSON格式的私钥文件。
* **接收方公钥**：接受者的钱包地址Pubkey（可以通过命令行参数传入并解析为`solana_sdk::pubkey::Pubkey`类型）。
* **转账金额**：以lamports为单位的u64数字。
* **交易数量N**：要发送的交易次数。
* **RPC节点列表**：包含多个节点的HTTP RPC URL和WebSocket URL。可以在配置中将每个节点表示为一个结构，如`{ rpc_http: "https://api.mainnet-beta.solana.com", rpc_ws: "wss://api.mainnet-beta.solana.com" }`。

配置读取完成后，程序进入主测试循环来生成并发送交易。

### 交易构建与签名

每次迭代生成一笔新的交易，共进行N次。对于每笔交易执行以下步骤：

1. **获取最新区块哈希**：使用Solana RPC接口获取当前最新的区块哈希（recent blockhash）。可以选择从提供的多个RPC节点中选一个作为获取blockhash的来源（例如列表中的第一个节点），以确保所用的blockhash与网络最新状态同步。调用Solana客户端提供的RPC方法，例如`RpcClient::get_latest_blockhash()`获取哈希。

2. **创建转账交易**：使用Solana SDK的工具构造一笔SOL转账交易。通过System Program的转账指令，将指定lamports从发送者转移给接收者。可以使用`solana_sdk::system_instruction::transfer(&from_pubkey, &to_pubkey, lamports)`构建转账指令，再将其加入交易。Solana SDK 提供了便捷方法，例如`solana_sdk::system_transaction::transfer(from_keypair, to_pubkey, lamports, recent_blockhash)` 来创建并签署一笔转账交易。展示了在Rust中构造System Program转账指令的用法。构造交易时需指定最近区块哈希和费率支付账户（默认为发送方）。然后使用发送方的Keypair对交易进行签名（solana\_sdk会在上述便捷函数中自动完成签名）。

3. **记录构建时间**：在获取blockhash和签名完成后，记录交易构建与签名所消耗的时间。例如，可在获取blockhash前记录起始时间戳，在交易签名完成后记录结束时间，两者差即为构建耗时。采用`std::time::Instant`进行高精度计时。

### 并发发送交易到多个节点

构建好的签名交易将被同时发送到多个RPC节点进行处理。为实现**并发发送**，我们使用Rust的异步运行时Tokio和Solana客户端的异步接口：

* **RpcClient初始化**：为每个RPC节点的HTTP地址创建一个RPC客户端对象。Solana的Rust SDK提供了非阻塞的RPC客户端实现，可直接用于异步环境。例如，使用`solana_rpc_client::nonblocking::rpc_client::RpcClient::new(node_url)` 创建异步客户端实例。这样可以在Tokio异步函数中直接调用`.await`来执行RPC请求。

* **并发请求发送**：利用Tokio任务或`Futures`接口，将交易发送请求分发到所有节点。可以通过Tokio的`join!`或`try_join!`宏，或者`futures::stream::FuturesUnordered`来并行await多个发送Future，从而同时等待所有节点的发送结果。每个RPC节点将收到交易的Base58编码并通过JSON-RPC的`sendTransaction`方法将交易提交到网络。

  **实现细节**：对于每个节点的发送，可调用`rpc_client.send_transaction(&transaction)`方法提交交易。如果需要，可以设置Commitment为`Processed`或`Confirmed`，但由于我们随后会自行确认，这里可以使用最低的确认级别以加快返回。在发送前记录时间戳，在获取发送结果后记录时间戳，以计算每个节点的发送延迟。

* **记录发送耗时和结果**：对于每个节点，保存发送耗时以及返回的结果（通常是交易签名或者错误）。正常情况下，各节点都会返回相同的交易签名（因为发送的是同一笔交易）。记录下每个节点从开始发送到收到响应的耗时，单位可以使用毫秒。

值得注意的是，由于我们将**同一笔交易**提交给多个节点，Solana网络会处理这笔交易一次，但各RPC节点的响应时间和确认感知可能不同。这种并行提交方式常用于测试不同节点的传播和确认速度。

### WebSocket确认订阅

在提交交易之后，我们需要跟踪每个节点对此交易的确认进度。Solana提供了RPC的WebSocket订阅接口，使客户端可以订阅特定交易签名的状态更新。当交易被确认或最终完成时，节点会通过WebSocket发送通知。我们的程序将为每个RPC节点建立WebSocket监听，以测量各节点确认该交易的速度。

* **建立WebSocket连接**：为提供的每个RPC节点的WebSocket地址建立连接。可以使用`tungstenite`/`tokio-tungstenite`库发起异步WebSocket连接，以便在Rust异步环境中监听消息。例如，通过`tokio_tungstenite::connect_async(node_ws_url).await?`连接，并获得一个WebSocket stream。也可以使用Solana SDK提供的Pubsub客户端简化订阅，例如`solana_client::pubsub_client::PubsubClient`的相关方法。

* **订阅交易签名**：连接建立后，发送订阅请求，订阅我们刚发送的交易签名的状态。使用`signatureSubscribe`方法，指定签名和所需的确认级别。我们选择**finalized**级别，以在交易完全最终确认时得到通知。发送的订阅请求为JSON格式，包括method为`signatureSubscribe`，参数为交易签名和`{"commitment":"finalized"}`。Solana RPC会返回一个订阅ID表示订阅成功。根据Solana WebSocket规范，我们可以同时对多个签名进行订阅，服务器会分别跟踪。因此，即使我们对每个节点发起多个订阅（针对每笔交易），都能同时收到各自的通知而互不影响。

* **异步监听通知**：订阅后，需要等待来自各节点的通知消息。对于每个节点，可以启动一个任务来读取该节点WebSocket连接的消息流。当收到`signatureNotification`通知且其中的状态达到`finalized`时，即表示该节点确认了此交易。Solana会在发送通知后自动取消该订阅，无需客户端手动取消。我们记录通知到达的时间戳，用于计算确认耗时。

  实现上，可以使用Tokio提供的异步Stream读取WebSocket消息，或使用`PubsubClient::signature_subscribe`简化处理。使用`PubsubClient::signature_subscribe(url, signature, Some(config))`会返回一个带有通道接收器的订阅句柄。我们可以从接收器阻塞等待消息。由于我们需要并发等待多个节点的通知，可能更灵活的做法是自行管理WebSocket连接并解析消息，以便将不同节点/签名的通知区分开来。

* **确认耗时与状态**：当收到某节点对于该交易的最终通知时，记录从最初发送交易到收到确认通知的时间差，作为该节点的确认延迟。与此同时，解析通知内容以确定最终状态是成功确认（一般会包含`result: { err: null }`表示成功）还是失败（err包含错误信息）。记录该状态用于输出。

### 性能数据记录

在上述流程中，我们需要采集分阶段的性能数据。为了准确记录时间，可以使用`std::time::Instant`在关键节点打点：

* `build_start`：开始获取blockhash前。
* `build_end`：交易构建并签名完成后。
* `send_start[node]`：对某节点开始发送交易前。
* `send_end[node]`：收到该节点发送请求响应后。
* `confirm[node]`：收到该节点最终确认通知时。

通过这些时间点计算：

* **构建签名耗时** = `build_end - build_start`。
* **发送耗时**（节点A） = `send_end[A] - send_start[A]` （对于每个节点分别计算）。
* **确认耗时**（节点A） = `confirm[A] - send_start[A]`（或用`send_end[A]`作为基准也可，近似差不多，以发送时刻为基准更统一）。

将上述数据临时存储在内存结构中，例如使用结构体或字典以交易签名为键，再附带每个节点的数据。当N笔交易都处理完毕后，再统一写出结果。

### 结果汇总与JSON输出

最后，程序将所有收集的性能指标按指定格式输出到JSON文件。为了便于分析，我们可以选择按交易划分或按节点划分结构。考虑到要求中每条记录需要包含节点URL和交易签名等信息，我们采用**逐交易逐节点**的列表形式输出，每个JSON对象代表某个节点处理某笔交易的结果。例如，输出JSON结构如下：

```json
[
  {
    "node": "https://rpc1.mainnet.solana.com",
    "tx_signature": "5Qk...abc", 
    "build_time_ms": 5,
    "send_time_ms": 30,
    "confirm_time_ms": 1500,
    "final_status": "Success"
  },
  {
    "node": "https://rpc2.mainnet.solana.com",
    "tx_signature": "5Qk...abc",
    "build_time_ms": 5,
    "send_time_ms": 45,
    "confirm_time_ms": 1600,
    "final_status": "Success"
  },
  ...
]
```

如上，每条交易的签名可能重复出现在多个条目中（对应不同节点）。我们也可以将结果按交易分组，但无论哪种方式，包含的信息应包括：

* **节点URL**：标识哪个RPC节点。
* **交易签名**：标识交易本身。
* **构建时间**：该笔交易构建和签名耗时（对同一交易而言，各节点相同）。
* **发送耗时**：该节点上交易提交的耗时。
* **确认耗时**：从提交到该节点报告交易finalized的总耗时。
* **最终状态**：交易是否成功确认。如成功则为`Success`（或记录确认slot等），如失败则包含错误类型。

使用Rust的**serde**库可以方便地序列化数据结构为JSON字符串，再输出到文件。

## 使用的工具和库

为实现上述功能，我们将使用以下Rust crates和技术：

* **Solana SDK (`solana-sdk`)**：提供`Keypair`、`Pubkey`、`Transaction`、`Instruction`等基础类型，用于构造和签名交易。例如用其SystemInstruction工具生成转账指令。
* **Solana Client (`solana-client`)**：包含与RPC交互的客户端实现。我们使用其非阻塞版本的RpcClient以适应Tokio异步运行。此外，`solana-client`附带了PubSub客户端，可用于WebSocket订阅（通过`solana_client::pubsub_client::PubsubClient`）提供方便的方法订阅交易状态。
* **Tokio 异步运行时**：用于启用异步并发。通过Tokio我们可以并行向多个节点发送交易请求，并同时监听多个异步WebSocket连接的数据。
* **Tokio-Tungstenite**：Tokio下的WebSocket库。若不使用Solana自带的PubsubClient，我们可以使用tokio-tungstenite手动连接RPC节点的WebSocket端口，并发送订阅请求，异步等待消息。
* **Solana Transaction Status**：`solana-transaction-status` crate 提供了交易状态相关的数据结构，例如`RpcSignatureResult`，可用于解析WebSocket通知中交易状态。这与PubsubClient配合，可直接获得Rust结构化的确认结果。
* **Serde/JSON**：用于序列化结果数据为JSON。我们会定义相应的数据结构（如`NodeResult`等）派生Serialize trait，然后输出到文件。

## 总结

通过以上设计，最终实现的Rust程序将能够对多个Solana RPC节点同时进行交易提交并跟踪确认，从而收集每个节点在交易处理各阶段的延迟数据。使用异步并发确保所有RPC节点同时接受交易并行处理。通过WebSocket订阅，我们能够及时获取每个节点的确认通知，以精确测量从发送到最终确认的时间差。表明在单一WebSocket连接上可以同时订阅多个签名的通知，这保证了即使我们对N笔交易在同一节点发起多个订阅也能可靠收到所有通知。输出的JSON文件汇总了每个节点对每笔交易的处理表现，便于后续分析哪个节点更快（发送延迟更低、确认更及时）以及错误率等，从而评估RPC节点的性能表现。

