import * as anchor from "@anchor-lang/core";
import { Program } from "@anchor-lang/core";
import { expect } from "chai";
import { readFileSync } from "node:fs";
import {
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_CLOCK_PUBKEY,
} from "@solana/web3.js";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  getAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  fetchAsset,
  fetchCollection,
  mplCore,
  transferV1,
  MPL_CORE_PROGRAM_ID,
} from "@metaplex-foundation/mpl-core";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import { keypairIdentity, publicKey } from "@metaplex-foundation/umi";
import { NftStaking } from "../target/types/nft_staking";

const DAY = 86_400;
const SCALE = 1_000_000n;
const BONUS = 1_000n * SCALE;

describe("Core NFT staking", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.nftStaking as Program<NftStaking>;
  const owner = provider.wallet.publicKey;
  const payer = (provider.wallet as anchor.Wallet).payer;
  const umi = createUmi(provider.connection.rpcEndpoint).use(mplCore());
  umi.use(
    keypairIdentity(umi.eddsa.createKeypairFromSecretKey(payer.secretKey))
  );
  const collection = Keypair.generate();
  const asset = Keypair.generate();
  const burnAsset = Keypair.generate();
  const attacker = Keypair.generate();
  const pda = (seed: string, address: PublicKey) =>
    PublicKey.findProgramAddressSync(
      [Buffer.from(seed), address.toBuffer()],
      program.programId
    )[0];
  const updateAuthority = pda("update_authority", collection.publicKey);
  const config = pda("config", collection.publicKey);
  const rewardsMint = pda("rewards_mint", config);
  const userRewardsAta = getAssociatedTokenAddressSync(rewardsMint, owner);
  const base = {
    owner,
    collection: collection.publicKey,
    config,
    updateAuthority,
    mplCoreProgram: MPL_CORE_PROGRAM_ID,
    systemProgram: SystemProgram.programId,
  };
  const rewardAccounts = (nft: PublicKey, tokenProgram = TOKEN_PROGRAM_ID) => ({
    ...base,
    asset: nft,
    stakePosition: pda("stake", nft),
    rewardsMint,
    userRewardsAta,
    tokenProgram,
    associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
  });
  const stakeAccounts = (nft: PublicKey) => ({
    ...base,
    asset: nft,
    stakePosition: pda("stake", nft),
  });
  const fetchedAsset = (nft = asset.publicKey) =>
    fetchAsset(umi, publicKey(nft.toBase58()));
  const position = (nft = asset.publicKey) =>
    program.account.stakePosition.fetch(pda("stake", nft));
  const balance = async () =>
    (await getAccount(provider.connection, userRewardsAta)).amount;
  const totalStaked = async () => {
    const data = await fetchCollection(
      umi,
      publicKey(collection.publicKey.toBase58())
    );
    const attrs = data.attributes?.attributeList ?? [];
    expect(
      attrs.filter((entry) => entry.key === "total_staked")
    ).to.have.length(1);
    return Number(attrs.find((entry) => entry.key === "total_staked")!.value);
  };
  const clock = async () =>
    Number(
      (await provider.connection.getAccountInfo(
        SYSVAR_CLOCK_PUBKEY
      ))!.data.readBigInt64LE(32)
    );
  const advanceTo = async (seconds: number) => {
    const result = (await fetch(provider.connection.rpcEndpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "surfnet_timeTravel",
        params: [{ absoluteTimestamp: seconds * 1000 }],
      }),
    }).then((response) => response.json())) as { error?: { message: string } };
    if (result.error) throw new Error(result.error.message);
  };
  const rewardsAt = (
    state: Awaited<ReturnType<typeof position>>,
    now: number
  ) =>
    (BigInt(now - state.lastClaimedAt.toNumber()) * 10_000n * SCALE +
      BigInt(state.rewardRemainder.toString())) /
    (BigInt(DAY) * 10_000n);
  const rejects = async (action: Promise<unknown>, code?: string) => {
    let failure: unknown;
    try {
      await action;
    } catch (error) {
      failure = error;
    }
    expect(failure, "transaction must fail").not.to.equal(undefined);
    if (code) {
      const error = failure as anchor.AnchorError;
      expect(error.error?.errorCode.code, String(failure)).to.equal(code);
    }
  };

  before(async () => {
    const endpoint = provider.connection.rpcEndpoint;
    if (
      !["localhost", "127.0.0.1", "[::1]"].includes(new URL(endpoint).hostname)
    ) {
      throw new Error(
        "This integration suite requires a local Surfpool RPC endpoint"
      );
    }
    // --skip-deploy starts Surfpool without our program. Load the current build
    // directly into its VM so no deployment transaction is needed.
    const loaded = (await fetch(endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        method: "surfnet_setAccount",
        params: [
          program.programId.toBase58(),
          {
            lamports: 1_000_000_000,
            data: readFileSync("target/deploy/nft_staking.so").toString("hex"),
            owner: "BPFLoader2111111111111111111111111111111111",
            executable: true,
          },
        ],
      }),
    }).then((response) => response.json())) as { error?: { message: string } };
    if (loaded.error) {
      throw new Error(
        `Cannot load the test program into Surfpool: ${loaded.error.message}`
      );
    }
    const programAccount = await provider.connection.getAccountInfo(
      program.programId
    );
    if (!programAccount?.executable) {
      throw new Error(
        "Surfpool did not load the staking program as executable"
      );
    }
    const coreAccount = await provider.connection.getAccountInfo(
      new PublicKey(MPL_CORE_PROGRAM_ID)
    );
    if (!coreAccount?.executable) {
      throw new Error(
        "Metaplex Core is unavailable; start Surfpool with its mainnet datasource"
      );
    }
    const signature = await provider.connection.requestAirdrop(
      attacker.publicKey,
      2_000_000_000
    );
    await provider.connection.confirmTransaction(signature, "confirmed");
  });

  it("creates collection Attributes with total_staked = 0", async () => {
    await program.methods
      .createCollection("Core Staking", "https://example.com/collection.json")
      .accountsPartial({
        payer: owner,
        collection: collection.publicKey,
        updateAuthority,
        mplCoreProgram: MPL_CORE_PROGRAM_ID,
      })
      .signers([collection])
      .rpc();
    expect(await totalStaked()).to.equal(0);
  });

  it("restricts config initialization to the collection creator", async () => {
    await rejects(
      program.methods
        .initialize(10_000, 7)
        .accountsPartial({
          admin: attacker.publicKey,
          collection: collection.publicKey,
          config,
          updateAuthority,
          rewardsMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([attacker])
        .rpc(),
      "InvalidAdmin"
    );
    expect(await provider.connection.getAccountInfo(config)).to.equal(null);
  });

  it("validates the reward rate and initializes a six-decimal reward mint", async () => {
    const accounts = {
      admin: owner,
      collection: collection.publicKey,
      config,
      updateAuthority,
      rewardsMint,
      tokenProgram: TOKEN_PROGRAM_ID,
    };
    await rejects(
      program.methods.initialize(10_001, 7).accountsPartial(accounts).rpc(),
      "InvalidRewardsBps"
    );
    await program.methods.initialize(10_000, 7).accountsPartial(accounts).rpc();
    expect((await program.account.config.fetch(config)).freezePeriod).to.equal(
      7
    );
  });

  it("restricts NFT issuance to the collection creator", async () => {
    const unauthorizedAsset = Keypair.generate();
    await rejects(
      program.methods
        .mintAsset("Unauthorized", "https://example.com/nft.json")
        .accountsPartial({
          user: attacker.publicKey,
          asset: unauthorizedAsset.publicKey,
          collection: collection.publicKey,
          updateAuthority,
          mplCoreProgram: MPL_CORE_PROGRAM_ID,
        })
        .signers([attacker, unauthorizedAsset])
        .rpc(),
      "InvalidAdmin"
    );
    expect(
      await provider.connection.getAccountInfo(unauthorizedAsset.publicKey)
    ).to.equal(null);
  });

  it("mints two NFTs and stakes them with FreezeDelegate and BurnDelegate", async () => {
    for (const nft of [asset, burnAsset]) {
      await program.methods
        .mintAsset("Staking NFT", "https://example.com/nft.json")
        .accountsPartial({
          user: owner,
          asset: nft.publicKey,
          collection: collection.publicKey,
          updateAuthority,
          mplCoreProgram: MPL_CORE_PROGRAM_ID,
        })
        .signers([nft])
        .rpc();
      await program.methods
        .stake()
        .accountsPartial(stakeAccounts(nft.publicKey))
        .rpc();
      const data = await fetchedAsset(nft.publicKey);
      expect(data.freezeDelegate?.frozen).to.equal(true);
      expect(data.burnDelegate?.authority.type).to.equal("Address");
      expect((await position(nft.publicKey)).owner.equals(owner)).to.equal(
        true
      );
    }
    expect(await totalStaked()).to.equal(2);
  });

  it("rejects duplicate stakes without inflating total_staked", async () => {
    await rejects(
      program.methods
        .stake()
        .accountsPartial(stakeAccounts(asset.publicKey))
        .rpc()
    );
    expect(await totalStaked()).to.equal(2);
  });

  it("prevents transfers while the NFT is frozen", async () => {
    await rejects(
      transferV1(umi, {
        asset: publicKey(asset.publicKey.toBase58()),
        collection: publicKey(collection.publicKey.toBase58()),
        newOwner: publicKey(attacker.publicKey.toBase58()),
      }).sendAndConfirm(umi)
    );
    expect((await fetchedAsset()).owner).to.equal(owner.toBase58());
  });

  it("rejects early claims, unstakes, and burns without changing state", async () => {
    await rejects(
      program.methods
        .claimRewards()
        .accountsPartial(rewardAccounts(asset.publicKey))
        .rpc(),
      "FreezePeriodNotOver"
    );
    await rejects(
      program.methods
        .unstake()
        .accountsPartial(rewardAccounts(asset.publicKey))
        .rpc(),
      "FreezePeriodNotOver"
    );
    await rejects(
      program.methods
        .burnStakedNft()
        .accountsPartial(rewardAccounts(burnAsset.publicKey))
        .rpc(),
      "FreezePeriodNotOver"
    );
    expect(await totalStaked()).to.equal(2);
    expect(
      (await fetchedAsset(burnAsset.publicKey)).freezeDelegate?.frozen
    ).to.equal(true);
  });

  it("rejects a different owner's claim and burn", async () => {
    const accounts = {
      ...rewardAccounts(asset.publicKey),
      owner: attacker.publicKey,
      userRewardsAta: getAssociatedTokenAddressSync(
        rewardsMint,
        attacker.publicKey
      ),
    };
    await rejects(
      program.methods
        .claimRewards()
        .accountsPartial(accounts)
        .signers([attacker])
        .rpc(),
      "InvalidOwner"
    );
    await rejects(
      program.methods
        .burnStakedNft()
        .accountsPartial(accounts)
        .signers([attacker])
        .rpc(),
      "InvalidOwner"
    );
  });

  it("claims into the user's ATA while preserving staking, freezing, and statistics", async () => {
    const before = await position();
    await advanceTo(before.stakedAt.toNumber() + 8 * DAY);
    await program.methods
      .claimRewards()
      .accountsPartial(rewardAccounts(asset.publicKey))
      .rpc();
    const after = await position();
    expect(await balance()).to.equal(
      rewardsAt(before, after.lastClaimedAt.toNumber())
    );
    expect(after.stakedAt.toString()).to.equal(before.stakedAt.toString());
    expect((await fetchedAsset()).freezeDelegate?.frozen).to.equal(true);
    expect(await totalStaked()).to.equal(2);
  });

  it("rejects redirecting claim or burn rewards to another wallet's ATA", async () => {
    const before = await position();
    const previousBalance = await balance();
    const accounts = {
      ...rewardAccounts(asset.publicKey),
      userRewardsAta: getAssociatedTokenAddressSync(
        rewardsMint,
        attacker.publicKey
      ),
    };
    await rejects(
      program.methods.claimRewards().accountsPartial(accounts).rpc()
    );
    await rejects(
      program.methods.burnStakedNft().accountsPartial(accounts).rpc()
    );
    expect(await balance()).to.equal(previousBalance);
    expect((await position()).lastClaimedAt.toString()).to.equal(
      before.lastClaimedAt.toString()
    );
    expect((await fetchedAsset()).freezeDelegate?.frozen).to.equal(true);
    expect(await totalStaked()).to.equal(2);
  });

  it("rejects an NFT supplied with a different valid collection", async () => {
    const other = Keypair.generate();
    const authority = pda("update_authority", other.publicKey);
    const otherConfig = pda("config", other.publicKey);
    const otherMint = pda("rewards_mint", otherConfig);
    await program.methods
      .createCollection("Other Collection", "https://example.com/other.json")
      .accountsPartial({
        payer: owner,
        collection: other.publicKey,
        updateAuthority: authority,
        mplCoreProgram: MPL_CORE_PROGRAM_ID,
      })
      .signers([other])
      .rpc();
    await program.methods
      .initialize(10_000, 0)
      .accountsPartial({
        admin: owner,
        collection: other.publicKey,
        updateAuthority: authority,
        config: otherConfig,
        rewardsMint: otherMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    const accounts = {
      ...rewardAccounts(asset.publicKey),
      collection: other.publicKey,
      config: otherConfig,
      updateAuthority: authority,
      rewardsMint: otherMint,
      userRewardsAta: getAssociatedTokenAddressSync(otherMint, owner),
    };
    await rejects(
      program.methods.claimRewards().accountsPartial(accounts).rpc(),
      "InvalidUpdateAuthority"
    );
    await rejects(
      program.methods.burnStakedNft().accountsPartial(accounts).rpc(),
      "InvalidUpdateAuthority"
    );
    expect((await fetchedAsset()).freezeDelegate?.frozen).to.equal(true);
    expect(await totalStaked()).to.equal(2);
    const otherData = await fetchCollection(
      umi,
      publicKey(other.publicKey.toBase58())
    );
    expect(
      otherData.attributes?.attributeList.find(
        (entry) => entry.key === "total_staked"
      )?.value
    ).to.equal("0");
  });

  it("a subsequent claim pays only newly accrued rewards", async () => {
    const before = await position();
    const previousBalance = await balance();
    await advanceTo(before.lastClaimedAt.toNumber() + DAY);
    await program.methods
      .claimRewards()
      .accountsPartial(rewardAccounts(asset.publicKey))
      .rpc();
    const after = await position();
    expect((await balance()) - previousBalance).to.equal(
      rewardsAt(before, after.lastClaimedAt.toNumber())
    );
    expect((await fetchedAsset()).freezeDelegate?.frozen).to.equal(true);
    expect(await totalStaked()).to.equal(2);
  });

  it("rolls back thawing, burning, and minting when collection accounting fails", async () => {
    const snapshot = (await provider.connection.getAccountInfo(
      collection.publicKey
    ))!;
    const corrupted = Buffer.from(snapshot.data);
    const key = Buffer.from("total_staked");
    const offset = corrupted.indexOf(key) + key.length + 4;
    expect(corrupted[offset]).to.equal("2".charCodeAt(0));
    corrupted[offset] = "0".charCodeAt(0);
    const setData = async (data: Buffer) => {
      const response = (await fetch(provider.connection.rpcEndpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "surfnet_setAccount",
          params: [
            collection.publicKey.toBase58(),
            { data: data.toString("hex") },
          ],
        }),
      }).then((result) => result.json())) as { error?: { message: string } };
      if (response.error) throw new Error(response.error.message);
    };
    const previousBalance = await balance();
    await setData(corrupted);
    try {
      await rejects(
        program.methods
          .burnStakedNft()
          .accountsPartial(rewardAccounts(burnAsset.publicKey))
          .rpc(),
        "InvalidCollectionStats"
      );
      expect(await balance()).to.equal(previousBalance);
      expect(
        (await fetchedAsset(burnAsset.publicKey)).freezeDelegate?.frozen
      ).to.equal(true);
      expect(
        (await position(burnAsset.publicKey)).asset.equals(burnAsset.publicKey)
      ).to.equal(true);
      expect(
        (await provider.connection.getAccountInfo(
          collection.publicKey
        ))!.data.equals(corrupted)
      ).to.equal(true);
    } finally {
      await setData(snapshot.data);
    }
    expect(await totalStaked()).to.equal(2);
  });

  it("burns through BurnDelegate, pays the 1,000-token bonus and pending rewards, and decrements the counter", async () => {
    const state = await position(burnAsset.publicKey);
    const previousBalance = await balance();
    const earliest = await clock();
    await program.methods
      .burnStakedNft()
      .accountsPartial(rewardAccounts(burnAsset.publicKey))
      .rpc();
    const latest = await clock();
    const paid = (await balance()) - previousBalance;
    expect(paid >= BONUS + rewardsAt(state, earliest)).to.equal(true);
    expect(paid <= BONUS + rewardsAt(state, latest)).to.equal(true);
    const burned = await provider.connection.getAccountInfo(
      burnAsset.publicKey
    );
    // Core keeps a one-byte Uninitialized tombstone, or the account is reclaimed.
    expect(
      burned === null || (burned.data.length === 1 && burned.data[0] === 0)
    ).to.equal(true);
    expect(
      await provider.connection.getAccountInfo(
        pda("stake", burnAsset.publicKey)
      )
    ).to.equal(null);
    expect(await totalStaked()).to.equal(1);
  });

  it("rejects another burn or claim of the destroyed NFT without another payout", async () => {
    const previousBalance = await balance();
    await rejects(
      program.methods
        .burnStakedNft()
        .accountsPartial(rewardAccounts(burnAsset.publicKey))
        .rpc()
    );
    await rejects(
      program.methods
        .claimRewards()
        .accountsPartial(rewardAccounts(burnAsset.publicKey))
        .rpc()
    );
    expect(await balance()).to.equal(previousBalance);
    expect(await totalStaked()).to.equal(1);
  });

  it("unstakes without restarting the original lock, removes delegates, and decrements total_staked", async () => {
    await program.methods
      .unstake()
      .accountsPartial(rewardAccounts(asset.publicKey))
      .rpc();
    const data = await fetchedAsset();
    expect(data.freezeDelegate).to.equal(undefined);
    expect(data.burnDelegate).to.equal(undefined);
    expect(
      await provider.connection.getAccountInfo(pda("stake", asset.publicKey))
    ).to.equal(null);
    expect(await totalStaked()).to.equal(0);
  });

  it("rejects claims and burns after unstaking", async () => {
    const previousBalance = await balance();
    await rejects(
      program.methods
        .claimRewards()
        .accountsPartial(rewardAccounts(asset.publicKey))
        .rpc()
    );
    await rejects(
      program.methods
        .burnStakedNft()
        .accountsPartial(rewardAccounts(asset.publicKey))
        .rpc()
    );
    expect(await balance()).to.equal(previousBalance);
    expect(await totalStaked()).to.equal(0);
  });

  it("restakes the same NFT and starts a new lock and reward period", async () => {
    await program.methods
      .stake()
      .accountsPartial(stakeAccounts(asset.publicKey))
      .rpc();
    const state = await position();
    expect(state.lastClaimedAt.toString()).to.equal(state.stakedAt.toString());
    expect(state.rewardRemainder.toString()).to.equal("0");
    expect((await fetchedAsset()).freezeDelegate?.frozen).to.equal(true);
    expect(await totalStaked()).to.equal(1);
    await rejects(
      program.methods
        .unstake()
        .accountsPartial(rewardAccounts(asset.publicKey))
        .rpc(),
      "FreezePeriodNotOver"
    );
    await advanceTo(state.stakedAt.toNumber() + 7 * DAY);
    await program.methods
      .unstake()
      .accountsPartial(rewardAccounts(asset.publicKey))
      .rpc();
    expect(await totalStaked()).to.equal(0);
  });

  it("supports Token-2022 reward ATAs as well as the original token program", async () => {
    const col = Keypair.generate();
    const nft = Keypair.generate();
    const authority = pda("update_authority", col.publicKey);
    const cfg = pda("config", col.publicKey);
    const mint = pda("rewards_mint", cfg);
    await program.methods
      .createCollection("Token-2022", "https://example.com/collection.json")
      .accountsPartial({
        payer: owner,
        collection: col.publicKey,
        updateAuthority: authority,
        mplCoreProgram: MPL_CORE_PROGRAM_ID,
      })
      .signers([col])
      .rpc();
    await program.methods
      .initialize(10_000, 0)
      .accountsPartial({
        admin: owner,
        collection: col.publicKey,
        updateAuthority: authority,
        config: cfg,
        rewardsMint: mint,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
      })
      .rpc();
    await program.methods
      .mintAsset("Token-2022 NFT", "https://example.com/nft.json")
      .accountsPartial({
        user: owner,
        collection: col.publicKey,
        asset: nft.publicKey,
        updateAuthority: authority,
        mplCoreProgram: MPL_CORE_PROGRAM_ID,
      })
      .signers([nft])
      .rpc();
    const accounts = {
      owner,
      collection: col.publicKey,
      asset: nft.publicKey,
      updateAuthority: authority,
      config: cfg,
      stakePosition: pda("stake", nft.publicKey),
      mplCoreProgram: MPL_CORE_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
    };
    await program.methods.stake().accountsPartial(accounts).rpc();
    const ata = getAssociatedTokenAddressSync(
      mint,
      owner,
      false,
      TOKEN_2022_PROGRAM_ID
    );
    const state = await position(nft.publicKey);
    await advanceTo(state.stakedAt.toNumber() + DAY);
    await program.methods
      .claimRewards()
      .accountsPartial({
        ...accounts,
        rewardsMint: mint,
        userRewardsAta: ata,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();
    expect(
      (
        await getAccount(
          provider.connection,
          ata,
          "confirmed",
          TOKEN_2022_PROGRAM_ID
        )
      ).amount >= SCALE
    ).to.equal(true);
    expect((await fetchedAsset(nft.publicKey)).freezeDelegate?.frozen).to.equal(
      true
    );
    await program.methods
      .burnStakedNft()
      .accountsPartial({
        ...accounts,
        rewardsMint: mint,
        userRewardsAta: ata,
        tokenProgram: TOKEN_2022_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .rpc();
    expect(
      (
        await getAccount(
          provider.connection,
          ata,
          "confirmed",
          TOKEN_2022_PROGRAM_ID
        )
      ).amount >=
        BONUS + SCALE
    ).to.equal(true);
    const data = await fetchCollection(
      umi,
      publicKey(col.publicKey.toBase58())
    );
    expect(
      data.attributes?.attributeList.find(
        (entry) => entry.key === "total_staked"
      )?.value
    ).to.equal("0");
  });
});
