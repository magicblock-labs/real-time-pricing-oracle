import * as anchor from "@coral-xyz/anchor";
import {Program, web3} from "@coral-xyz/anchor";
import { EphemeralOracle } from "../target/types/ephemeral_oracle";

describe("ephemeral-oracle", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const provider = anchor.getProvider() as anchor.AnchorProvider;
  const program = anchor.workspace.EphemeralOracle as Program<EphemeralOracle>;

  const exampleFeedAddress = web3.PublicKey.findProgramAddressSync([Buffer.from("price_feed"), Buffer.from("stork-oracle"), Buffer.from("SOLUSD")], program.programId)[0];
  // 6 == SOLUSD, from https://history.pyth-lazer.dourolabs.app/history/v1/symbols
  // 1 == BTCUSD
  // 2 == ETHUSD
  // 7 == USDCUSD
  const exampleFeedAddress2 = web3.PublicKey.findProgramAddressSync([Buffer.from("price_feed"), Buffer.from("pyth-lazer"), Buffer.from("6")], program.programId)[0];

  const providerEphemeralRollup = new anchor.AnchorProvider(
      new anchor.web3.Connection(
          process.env.PROVIDER_ENDPOINT || "https://devnet.magicblock.app/",
          {
            wsEndpoint: process.env.WS_ENDPOINT || "wss://devnet.magicblock.app/",
          }
      ),
      anchor.Wallet.local()
  );
  const ephemeralProgram = new Program(program.idl, providerEphemeralRollup);

  it("Initialize price feed!", async () => {
    const tx = await program.methods.initializePriceFeed("stork-oracle", "SOLUSD", Array.from(exampleFeedAddress.toBytes()), 18).accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc({skipPreflight: true});
    console.log("Initialize price feed signature", tx);
  });

  it("Initialize price feed 2!", async () => {
    const tx = await program.methods.initializePriceFeed("pyth-lazer", "1", Array.from(exampleFeedAddress2.toBytes()), 8).accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc();
    console.log("Initialize price feed signature", tx);
  });

  it("Update price feed!", async () => {
    const updateData = {
      symbol: "SOLUSD",
      id: Array(32).fill(0),
      temporalNumericValue: {
        timestampNs: new anchor.BN(Date.now()),
        quantizedValue: new anchor.BN(1000000)
      },
      publisherMerkleRoot: Array(32).fill(0),
      valueComputeAlgHash: Array(32).fill(0),
      r: Array(32).fill(0),
      s: Array(32).fill(0),
      v: 0,
    };
    const tx = await program.methods.updatePriceFeed("stork-oracle", updateData).accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc();
    console.log("Update price feed signature", tx);
  });

  it("Delegate price feed 1!", async () => {
    const tx = await program.methods.delegatePriceFeed("stork-oracle", "SOLUSD").accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc();
    console.log("Delegate price feed signature", tx);
  });

  it("Delegate price feed 2!", async () => {
    const tx = await program.methods.delegatePriceFeed("pyth-lazer", "6").accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc();
    console.log("Delegate price feed signature", tx);
  });

  it("Undelegate price feed!", async () => {
    const tx = await ephemeralProgram.methods.undelegatePriceFeed("pyth-lazer", "2").accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: exampleFeedAddress2,
    }).rpc();
    console.log("Delegate price feed signature", tx);
  });

  it("Close price feed!", async () => {
    const tx = await program.methods.closePriceFeed("pyth-lazer", "2").accounts({
      payer: anchor.getProvider().publicKey,
    }).rpc({skipPreflight: true});
    console.log("Close price feed signature", tx);
  });

  it("Update price feed delegated!", async () => {
    const updateData = {
      symbol: "SOLUSD",
      id: Array.from(exampleFeedAddress.toBytes()),
      temporalNumericValue: {
        timestampNs: new anchor.BN(Date.now()),
        quantizedValue: new anchor.BN(1000000)
      },
      publisherMerkleRoot: Array(32).fill(0),
      valueComputeAlgHash: Array(32).fill(0),
      r: Array(32).fill(0),
      s: Array(32).fill(0),
      v: 0,
    };
    const tx = await ephemeralProgram.methods.updatePriceFeed("stork-oracle", updateData).accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: exampleFeedAddress,
    }).rpc();
    console.log("Update price feed signature", tx);
  });

  it("Update price feed delegated 2!", async () => {
    const updateData = {
      symbol: "6",
      id: Array.from(exampleFeedAddress2.toBytes()),
      temporalNumericValue: {
        timestampNs: new anchor.BN(Date.now()),
        quantizedValue: new anchor.BN(1000000)
      },
      publisherMerkleRoot: Array(32).fill(0),
      valueComputeAlgHash: Array(32).fill(0),
      r: Array(32).fill(0),
      s: Array(32).fill(0),
      v: 4
    };
    const tx = await ephemeralProgram.methods.updatePriceFeed("pyth-lazer", updateData).accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: exampleFeedAddress2,
    }).rpc();
    console.log("Update price feed signature", tx);
  });

  it("Get SOL/USD price from Stork!", async () => {
    const tx = await program.methods.sample().accounts({
      priceUpdate: exampleFeedAddress,
    }).rpc();
    console.log("Use price transaction   signature", tx);
  });

  it("Get SOL/USD price Pyth!", async () => {
    console.log("price feed id", exampleFeedAddress2.toBase58());
    const tx = await ephemeralProgram.methods.sample().accounts({
      priceUpdate: exampleFeedAddress2,
    }).rpc();
    providerEphemeralRollup.connection.getAccountInfo(exampleFeedAddress2).then((data) => {
      const decodedData = program.account.priceUpdateV3.coder.accounts.decode("priceUpdateV3", Buffer.from(data.data));
      console.log("Price", decodedData.priceMessage.price.toString() / 10 ** decodedData.priceMessage.exponent);
    });
    console.log("Your transaction signature", tx);
  });

  it.skip("Initialize all the price feeds!", async () => {
    // Fetch symbols from Pyth Lazer API
    const response = await fetch("https://history.pyth-lazer.dourolabs.app/history/v1/symbols");
    const symbols = await response.json();

    // @ts-ignore
    console.log(`Found ${symbols.length} symbols from Pyth Lazer API`);

    // Initialize each price feed
    // @ts-ignore
    for (const symbol of symbols.slice(0, 100)) {
      const pythLazerId = symbol.pyth_lazer_id.toString();
      const pythName = symbol.name.toString();

      const feedAddress = web3.PublicKey.findProgramAddressSync(
          [Buffer.from("price_feed"), Buffer.from("pyth-lazer"), Buffer.from(pythLazerId)],
          program.programId
      )[0];

      console.log(`Initializing price feed for symbol ${pythLazerId}, pyth name ${pythName}, address: ${feedAddress.toBase58()}`);

      let acc = await provider.connection.getAccountInfo(feedAddress);

      if (acc && acc.owner.toBase58() == "DELeGGvXpWV2fqJUhqcF5ZSYMS4JTLjteaAMARRSaeSh") {
        // Undelegate the price feed
        console.log(`Undelegate price feed for symbol ${pythLazerId}`);
        await ephemeralProgram.methods.undelegatePriceFeed("pyth-lazer", pythLazerId).accounts({
          payer: anchor.getProvider().publicKey,
          priceFeed: feedAddress,
        }).rpc();
        await new Promise(resolve => setTimeout(resolve, 3000));
      }

      if(acc && acc.owner != web3.SystemProgram.programId) {
        // Close the price feed
        await program.methods.closePriceFeed("pyth-lazer", pythLazerId).accounts({
          payer: anchor.getProvider().publicKey,
        }).rpc();
      }

      // Initialize the price feed
      const tx = await program.methods.initializePriceFeed("pyth-lazer", pythLazerId, Array.from(feedAddress.toBytes()), 8).accounts({
        payer: anchor.getProvider().publicKey,
      }).rpc();
      console.log(`Initialized price feed for symbol ${pythLazerId}, tx: ${tx}`);

      // Delegate the price feed
      const tx2 = await program.methods.delegatePriceFeed("pyth-lazer", pythLazerId).accounts({
        payer: anchor.getProvider().publicKey,
      }).rpc();
      console.log(`Delegated price feed for symbol ${pythLazerId}, tx: ${tx2}`);
    }
  });

});
