// --- Wallet State ---
let walletPublicKey = null;
let ownedCards = [];
let burnedMints = new Set(JSON.parse(localStorage.getItem('burnedMints') || '[]'));

// --- Phantom Wallet Connection ---

function getPhantom() {
    if (window.phantom?.solana?.isPhantom) return window.phantom.solana;
    return null;
}

async function connectWallet() {
    const phantom = getPhantom();
    if (!phantom) {
        showOverlay(`
            <h2>Phantom Not Found</h2>
            <p>Install the <a href="https://phantom.app/" target="_blank" style="color:#c9a84c">Phantom wallet</a> extension to use NFT features.</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
        return;
    }

    try {
        const resp = await phantom.connect();
        walletPublicKey = resp.publicKey.toString();
        updateWalletUI();
        await refreshOwnedCards();
    } catch (e) {
        console.error('Wallet connect failed:', e);
    }
}

async function disconnectWallet() {
    const phantom = getPhantom();
    if (phantom) {
        try { await phantom.disconnect(); } catch (_) {}
    }
    walletPublicKey = null;
    ownedCards = [];
    updateWalletUI();
}

function updateWalletUI() {
    const btn = document.getElementById('wallet-btn');
    const storeBtn = document.getElementById('store-btn');
    const collectionBtn = document.getElementById('collection-btn');

    if (walletPublicKey) {
        const short = walletPublicKey.slice(0, 4) + '...' + walletPublicKey.slice(-4);
        btn.textContent = short;
        btn.onclick = disconnectWallet;
        if (storeBtn) storeBtn.classList.remove('hidden');
        if (collectionBtn) collectionBtn.classList.remove('hidden');
    } else {
        btn.textContent = 'Connect Wallet';
        btn.onclick = connectWallet;
        if (storeBtn) storeBtn.classList.add('hidden');
        if (collectionBtn) collectionBtn.classList.add('hidden');
    }
}

// --- Owned Cards ---

async function refreshOwnedCards() {
    if (!walletPublicKey) return;
    try {
        const data = await api('POST', '/api/wallet/cards', {
            wallet_address: walletPublicKey,
        });
        const fresh = data.cards || [];
        // If DAS no longer returns a burned mint, it's fully indexed — stop filtering it
        for (const mint of burnedMints) {
            if (!fresh.some(c => c.mint_address === mint)) {
                burnedMints.delete(mint);
            }
        }
        localStorage.setItem('burnedMints', JSON.stringify([...burnedMints]));
        ownedCards = fresh.filter(c => !burnedMints.has(c.mint_address));
    } catch (e) {
        console.error('Failed to fetch owned cards:', e);
        ownedCards = [];
    }
}

// --- Transaction Signing ---

async function signAndSubmitTransaction(base64Tx) {
    const phantom = getPhantom();
    if (!phantom) throw new Error('Phantom not available');

    // Decode base64 to bytes
    const bytes = Uint8Array.from(atob(base64Tx), c => c.charCodeAt(0));

    // Deserialize as a versioned transaction
    const tx = solanaWeb3.Transaction.from(bytes);

    // User signs via Phantom
    const signed = await phantom.signTransaction(tx);

    // Serialize back to base64
    const signedBytes = signed.serialize();
    const signedBase64 = btoa(String.fromCharCode(...signedBytes));

    // Submit via backend
    const result = await api('POST', '/api/wallet/submit-tx', {
        signed_transaction: signedBase64,
    });

    return result.signature;
}

// --- Claim Card as NFT ---

async function claimCard(cardId, gameId) {
    if (!walletPublicKey) {
        showOverlay(`
            <h2>Wallet Required</h2>
            <p>Connect your Phantom wallet to claim cards as NFTs.</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
        return;
    }

    try {
        showLoading('Building claim transaction...');
        const data = await api('POST', '/api/wallet/claim', {
            wallet_address: walletPublicKey,
            card_id: cardId,
            game_id: gameId,
        });
        hideLoading();

        showLoading('Confirm in Phantom...');
        const sig = await signAndSubmitTransaction(data.transaction);
        hideLoading();

        await refreshOwnedCards();

        showOverlay(`
            <h2>NFT Claimed!</h2>
            <p><strong>${data.card.name}</strong> has been minted to your wallet.</p>
            <p style="font-size:0.7rem;color:#6a5a40;word-break:break-all">Signature: ${sig}</p>
            <button onclick="hideOverlay()">Continue</button>
        `);
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Claim Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

// --- Pack Purchase ---

async function buyPack(packType) {
    if (!walletPublicKey) return;

    try {
        showLoading('Preparing pack...');
        const data = await api('POST', '/api/wallet/pack/buy', {
            wallet_address: walletPublicKey,
            pack_type: packType,
        });
        hideLoading();

        // User signs one payment transaction
        showLoading('Confirm payment in Phantom...');
        const paymentSig = await signAndSubmitTransaction(data.payment_transaction);
        hideLoading();

        // Server mints all cards
        showLoading('Minting cards...');
        await api('POST', '/api/wallet/pack/confirm', {
            payment_signature: paymentSig,
            wallet_address: walletPublicKey,
            pack_cards: data.pack_cards,
        });
        hideLoading();

        await refreshOwnedCards();
        await showPackReveal(data.cards);
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Purchase Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

async function showPackReveal(cards) {
    for (let i = 0; i < cards.length; i++) {
        await showCardReveal(cards[i]);
    }
    showOverlay(`
        <h2>Pack Complete!</h2>
        <p>You received ${cards.length} cards. Check your collection!</p>
        <button onclick="hideOverlay()">Continue</button>
    `);
}

// --- Store Screen ---

function showStore() {
    showScreen('store-screen');
}

function closeStore() {
    showScreen('title-screen');
}

// --- Collection Screen ---

async function showCollection() {
    showScreen('collection-screen');
    console.log('showCollection: wallet =', walletPublicKey);
    await refreshOwnedCards();
    console.log('showCollection: ownedCards =', ownedCards);
    renderCollection();
}

function closeCollection() {
    showScreen('title-screen');
}

let combineMode = false;
let selectedForCombine = new Set();

function renderCollection() {
    const grid = document.getElementById('collection-grid');
    if (!grid) return;
    grid.innerHTML = '';

    if (ownedCards.length === 0) {
        grid.innerHTML = '<p style="color:#6a5a40;grid-column:1/-1;text-align:center">No cards yet. Buy packs to get started!</p>';
        return;
    }

    ownedCards.forEach((card, i) => {
        const div = document.createElement('div');
        div.className = 'collection-card';
        if (combineMode && selectedForCombine.has(i)) {
            div.classList.add('selected');
        }

        const img = document.createElement('img');
        img.src = card.image_path || '/cards/placeholder.png';
        img.alt = card.name;
        div.appendChild(img);

        const name = document.createElement('div');
        name.className = 'collection-card-name';
        name.textContent = card.name;
        div.appendChild(name);

        const kind = document.createElement('div');
        kind.className = 'collection-card-kind ' + card.kind;
        kind.textContent = card.kind;
        div.appendChild(kind);

        if (combineMode) {
            div.onclick = () => toggleCombineSelect(i);
        }

        grid.appendChild(div);
    });

    updateCombineButton();
}

function toggleCombineMode() {
    combineMode = !combineMode;
    selectedForCombine.clear();
    const btn = document.getElementById('combine-mode-btn');
    if (btn) btn.textContent = combineMode ? 'Cancel' : 'Combine Cards';
    renderCollection();
}

function toggleCombineSelect(index) {
    if (selectedForCombine.has(index)) {
        selectedForCombine.delete(index);
    } else {
        if (selectedForCombine.size >= 4) return;
        selectedForCombine.add(index);
    }
    renderCollection();
}

function updateCombineButton() {
    const btn = document.getElementById('do-combine-btn');
    if (!btn) return;
    if (combineMode && selectedForCombine.size >= 2) {
        btn.classList.remove('hidden');
        btn.disabled = false;
    } else {
        btn.classList.add('hidden');
    }
}

async function combineOwnedCards() {
    if (!walletPublicKey || selectedForCombine.size < 2) return;

    const mintAddresses = Array.from(selectedForCombine).map(i => ownedCards[i].mint_address);

    try {
        showLoading('Checking combination...');
        const data = await api('POST', '/api/wallet/combine', {
            wallet_address: walletPublicKey,
            mint_addresses: mintAddresses,
        });
        hideLoading();

        showLoading('Confirm in Phantom...');
        const sig = await signAndSubmitTransaction(data.transaction);
        hideLoading();

        // Optimistically remove burned cards and add the new one
        // (DAS indexing lags behind on-chain state)
        for (const m of mintAddresses) burnedMints.add(m);
        localStorage.setItem('burnedMints', JSON.stringify([...burnedMints]));
        ownedCards = ownedCards.filter(c => !burnedMints.has(c.mint_address));
        ownedCards.push({
            mint_address: data.asset_address,
            card_id: data.card.card_id,
            name: data.card.name,
            description: data.card.description,
            image_path: data.card.image_path,
            kind: 'crafted',
        });

        combineMode = false;
        selectedForCombine.clear();

        await showCardReveal(data.card);
        renderCollection();
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Combine Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

// --- NFT Card Selection for Game Start ---

async function showNftSelection() {
    if (!walletPublicKey || ownedCards.length === 0) return null;

    return new Promise(resolve => {
        const overlay = document.createElement('div');
        overlay.className = 'card-reveal-overlay';
        const selected = new Set();

        function renderSelectionGrid() {
            overlay.innerHTML = `
                <div class="reveal-title">Select NFT Cards (up to 4)</div>
                <div class="nft-select-grid">
                    ${ownedCards.map((card, i) => `
                        <div class="nft-select-card ${selected.has(i) ? 'selected' : ''}" data-idx="${i}">
                            <img src="${card.image_path || ''}" alt="${card.name}">
                            <div class="nft-select-name">${card.name}</div>
                        </div>
                    `).join('')}
                </div>
                <div class="nft-select-actions">
                    <button id="nft-skip-btn">Skip</button>
                    <button id="nft-confirm-btn" ${selected.size === 0 ? 'disabled' : ''}>
                        Use ${selected.size} Card${selected.size !== 1 ? 's' : ''}
                    </button>
                </div>
            `;

            overlay.querySelectorAll('.nft-select-card').forEach(div => {
                div.onclick = () => {
                    const idx = parseInt(div.dataset.idx);
                    if (selected.has(idx)) {
                        selected.delete(idx);
                    } else if (selected.size < 4) {
                        selected.add(idx);
                    }
                    renderSelectionGrid();
                };
            });

            overlay.querySelector('#nft-skip-btn').onclick = () => {
                overlay.remove();
                resolve(null);
            };

            overlay.querySelector('#nft-confirm-btn').onclick = () => {
                const cards = Array.from(selected).map(i => ({
                    mint_address: ownedCards[i].mint_address,
                    card_id: ownedCards[i].card_id,
                }));
                overlay.remove();
                resolve(cards);
            };
        }

        renderSelectionGrid();
        document.body.appendChild(overlay);
    });
}

// --- Auto-connect on page load ---

window.addEventListener('load', async () => {
    const phantom = getPhantom();
    if (phantom) {
        try {
            const resp = await phantom.connect({ onlyIfTrusted: true });
            walletPublicKey = resp.publicKey.toString();
            updateWalletUI();
            refreshOwnedCards();
        } catch (_) {
            // Not previously connected
        }
    }
});
