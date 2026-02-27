#!/usr/bin/env node
/**
 * One-time script to create a Metaplex Core collection for Alchemaybe.
 *
 * Prerequisites:
 *   npm install @metaplex-foundation/mpl-core @metaplex-foundation/umi
 *   npm install @metaplex-foundation/umi-bundle-defaults @solana/web3.js
 *
 * Usage:
 *   SOLANA_KEYPAIR_PATH=~/.config/solana/id.json \
 *   SOLANA_RPC_URL=https://devnet.helius-rpc.com/?api-key=YOUR_KEY \
 *   node scripts/create-collection.js
 *
 * Output: prints the collection address to use as COLLECTION_ADDRESS env var.
 */

const { createUmi } = require('@metaplex-foundation/umi-bundle-defaults');
const { createCollectionV1 } = require('@metaplex-foundation/mpl-core');
const { generateSigner, keypairIdentity } = require('@metaplex-foundation/umi');
const fs = require('fs');

async function main() {
    const keypairPath = process.env.SOLANA_KEYPAIR_PATH;
    const rpcUrl = process.env.SOLANA_RPC_URL;

    if (!keypairPath || !rpcUrl) {
        console.error('Set SOLANA_KEYPAIR_PATH and SOLANA_RPC_URL env vars');
        process.exit(1);
    }

    const secretKey = JSON.parse(fs.readFileSync(keypairPath, 'utf8'));

    const umi = createUmi(rpcUrl);

    // Import the keypair
    const keypair = umi.eddsa.createKeypairFromSecretKey(new Uint8Array(secretKey));
    umi.use(keypairIdentity(keypair));

    const collectionSigner = generateSigner(umi);

    console.log('Creating Alchemaybe collection...');
    console.log('Authority:', keypair.publicKey);

    const tx = await createCollectionV1(umi, {
        collection: collectionSigner,
        name: 'Alchemaybe Cards',
        uri: '', // Can be updated later with full collection metadata
    }).sendAndConfirm(umi);

    console.log('\nCollection created!');
    console.log('Collection address:', collectionSigner.publicKey);
    console.log('\nAdd to your .env:');
    console.log(`COLLECTION_ADDRESS=${collectionSigner.publicKey}`);
}

main().catch(err => {
    console.error('Failed:', err);
    process.exit(1);
});
