import * as anchor from "@coral-xyz/anchor";
import {Program, web3} from "@coral-xyz/anchor";
import { EphemeralOracle } from "../target/types/ephemeral_oracle";

describe("ephemeral-oracle", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.EphemeralOracle as Program<EphemeralOracle>;

  const feedAddress = web3.PublicKey.findProgramAddressSync([Buffer.from("price_feed"), Buffer.from("stork"), Buffer.from("SOLUSD")], program.programId)[0];

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
    const feedAddress = web3.PublicKey.findProgramAddressSync([Buffer.from("price_feed"), Buffer.from("stork"), Buffer.from("SOLUSD")], program.programId)[0];
    const tx = await program.methods.initializePriceFeed("stork", "SOLUSD", Array.from(feedAddress.toBytes()), 18).accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: feedAddress,
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
      v: 0
    };
    const tx = await program.methods.updatePriceFeed("stork", updateData).accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: feedAddress,
    }).rpc();
    console.log("Update price feed signature", tx);
  });

  it("Delegate price feed!", async () => {
    const tx = await program.methods.delegatePriceFeed("stork", "SOLUSD").accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: feedAddress,
    }).rpc();
    console.log("Delegate price feed signature", tx);
  });

  it("Update price feed delegated!", async () => {
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
      v: 0
    };
    const tx = await ephemeralProgram.methods.updatePriceFeed("stork", updateData).accounts({
      payer: anchor.getProvider().publicKey,
      priceFeed: feedAddress,
    }).rpc();
    console.log("Update price feed signature", tx);
  });

  it.only("Get price!", async () => {
    const tx = await ephemeralProgram.methods.sample().accounts({
      priceUpdate: feedAddress,
    }).rpc();
    console.log("Your transaction signature", tx);
  });

});
