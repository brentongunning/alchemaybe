let gameId = null;
let gameState = null;
let selectedHandIndices = new Set();

// --- API helpers ---

async function api(method, path, body) {
    const opts = { method, headers: { 'Content-Type': 'application/json' } };
    if (body) opts.body = JSON.stringify(body);
    const resp = await fetch(path, opts);
    const data = await resp.json();
    if (!resp.ok) throw new Error(data.error || data.reason || 'Request failed');
    return data;
}

// --- Screen management ---

function showScreen(id) {
    document.querySelectorAll('.screen').forEach(s => s.classList.remove('active'));
    document.getElementById(id).classList.add('active');
}

function showLoading(text) {
    document.getElementById('loading-text').textContent = text || 'Working...';
    document.getElementById('loading').classList.remove('hidden');
}

function hideLoading() {
    document.getElementById('loading').classList.add('hidden');
}

function showOverlay(html) {
    document.getElementById('overlay-content').innerHTML = html;
    document.getElementById('overlay').classList.remove('hidden');
}

function hideOverlay() {
    document.getElementById('overlay').classList.add('hidden');
}

// --- Rules popup ---

function showRules() {
    showOverlay(`
        <h2>How to Play</h2>
        <div class="rules-text">
            <p><strong>Goal:</strong> Fill 5 cells on the 3x3 board. First to 5 wins.</p>

            <p><strong>Your hand</strong> has material cards (Fire, Stone, Wood...) and
            intent cards (Sharp, Hollow, Ancient...). You draw back to 7 at the end of each turn.</p>

            <p><strong>On your turn, you can:</strong></p>
            <ul>
                <li><strong>Combine</strong> — Select 2-4 cards and hit Combine.
                At least 1 must be a material. You can add 1 intent to
                guide what gets created. The result is a new crafted card
                that goes into your hand. You can combine multiple times per turn!</li>
                <li><strong>Place</strong> — Click a board cell to place a crafted card (one per turn).
                Pick a cell whose category matches your card.
                If an opponent owns the cell, a judge decides which
                card fits the category better. If you fail, you keep your card.</li>
                <li><strong>Discard</strong> — Select 1-3 cards and hit Discard to
                toss bad cards. You'll draw replacements at end of turn.</li>
                <li><strong>End Turn</strong> — Pass to the next player. Your hand
                refills to 7 cards.</li>
            </ul>

            <p><strong>Tips:</strong> Crafted cards can be combined again with other
            materials. Not every combination works — the alchemist decides!
            Think about the board categories when choosing what to craft.</p>
        </div>
        <button onclick="hideOverlay()">Got it</button>
    `);
}

// --- New card reveal animation ---

function showCardReveal(card, opts) {
    const canClaim = opts?.canClaim && walletPublicKey;
    const cardId = opts?.cardId || card.card_id || card.id;
    return new Promise(resolve => {
        const overlay = document.createElement('div');
        overlay.className = 'card-reveal-overlay';
        overlay.innerHTML = `
            <div class="reveal-title">New Discovery</div>
            <div class="card-reveal"><img src="${card.image_path}" alt="${card.name}"></div>
            <div class="reveal-name">${card.name}</div>
            <div class="reveal-desc">${card.description}</div>
            ${canClaim ? `<button class="claim-nft-btn" id="claim-nft-btn">Claim as NFT</button>` : ''}
            <div class="reveal-dismiss">Click anywhere to continue</div>
        `;
        const dismiss = () => {
            overlay.remove();
            resolve();
        };
        overlay.onclick = (e) => {
            if (e.target.id === 'claim-nft-btn') return;
            dismiss();
        };
        if (canClaim) {
            setTimeout(() => {
                const claimBtn = overlay.querySelector('#claim-nft-btn');
                if (claimBtn) {
                    claimBtn.onclick = async (e) => {
                        e.stopPropagation();
                        overlay.remove();
                        await claimCard(cardId, gameId);
                        resolve();
                    };
                }
            }, 0);
        }
        document.body.appendChild(overlay);
    });
}

// --- Game start ---

async function startGame(mode) {
    try {
        // Offer NFT selection if wallet connected and has cards
        let nftCards = [];
        let walletAddr = null;
        if (walletPublicKey && ownedCards.length > 0) {
            const selection = await showNftSelection();
            if (selection && selection.length > 0) {
                nftCards = selection;
                walletAddr = walletPublicKey;
            }
        }

        showLoading('Creating game...');
        const body = { mode };
        if (walletAddr) body.wallet_address = walletAddr;
        if (nftCards.length > 0) body.nft_cards = nftCards;
        gameState = await api('POST', '/api/game/new', body);
        gameId = gameState.id;
        document.getElementById('p2-label').textContent = mode === 'bot' ? 'Bot' : 'Player 2';
        showScreen('game-screen');
        render();
    } catch (e) {
        showOverlay(`
            <h2>Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    } finally {
        hideLoading();
    }
}

// --- Rendering ---

function render() {
    if (!gameState) return;
    renderBoard();
    renderHand();
    renderStatus();
}

function renderBoard() {
    const board = document.getElementById('board');
    board.innerHTML = '';
    for (let r = 0; r < 3; r++) {
        for (let c = 0; c < 3; c++) {
            const cell = gameState.board[r][c];
            const div = document.createElement('div');
            div.className = 'board-cell';
            if (cell.card) {
                div.classList.add('owner-' + cell.card.owner);
            }
            div.onclick = () => placeCard(r, c);

            const label = document.createElement('div');
            label.className = 'category-label';
            label.textContent = cell.category;
            div.appendChild(label);

            if (cell.card) {
                const img = document.createElement('img');
                img.className = 'cell-card';
                img.src = cell.card.card.image_path;
                img.alt = cell.card.card.name;
                div.appendChild(img);

                const badge = document.createElement('div');
                badge.className = 'owner-badge';
                const ownerName = cell.card.owner === 0 ? 'P1' : (gameState.mode === 'bot' ? 'Bot' : 'P2');
                badge.textContent = ownerName;
                div.appendChild(badge);
            }

            board.appendChild(div);
        }
    }
}

function renderHand() {
    const hand = document.getElementById('hand');
    hand.innerHTML = '';

    const isMyTurn = gameState.phase !== 'game_over' &&
        (gameState.mode !== 'bot' || gameState.current_player === 0);

    const player = gameState.players[isMyTurn ? gameState.current_player : 0];

    if (!isMyTurn && gameState.phase !== 'game_over') {
        hand.innerHTML = '<p style="color: #6a5a40; font-size: 0.8rem;">Waiting for opponent...</p>';
        document.getElementById('combine-btn').disabled = true;
        document.getElementById('discard-btn').disabled = true;
        document.getElementById('end-turn-btn').disabled = true;
        return;
    }

    const gameOver = gameState.phase === 'game_over';
    document.getElementById('end-turn-btn').disabled = gameOver;

    player.hand.forEach((card, i) => {
        const div = document.createElement('div');
        div.className = 'hand-card';
        if (card.kind === 'intent') div.classList.add('intent');
        if (card.kind === 'crafted') div.classList.add('crafted');
        if (selectedHandIndices.has(i)) div.classList.add('selected');
        div.onclick = () => toggleHandCard(i);

        const img = document.createElement('img');
        img.src = card.image_path;
        img.alt = card.name;
        div.appendChild(img);

        if (card.nft_mint) {
            const badge = document.createElement('div');
            badge.className = 'card-badge nft-badge';
            badge.textContent = 'NFT';
            div.appendChild(badge);
        } else if (card.kind === 'crafted') {
            const badge = document.createElement('div');
            badge.className = 'card-badge';
            badge.textContent = 'crafted';
            div.appendChild(badge);
        }

        const name = document.createElement('div');
        name.className = 'card-name';
        name.textContent = card.name;
        div.appendChild(name);

        hand.appendChild(div);
    });

    updateButtons();
}

function renderStatus() {
    document.getElementById('score-0').textContent = gameState.players[0].score;
    document.getElementById('score-1').textContent = gameState.players[1].score;

    const p1 = document.querySelector('.player-info.p1');
    const p2 = document.querySelector('.player-info.p2');
    p1.classList.toggle('active', gameState.current_player === 0);
    p2.classList.toggle('active', gameState.current_player === 1);

    const turnInfo = document.getElementById('turn-info');
    if (gameState.phase === 'game_over') {
        const winnerName = gameState.winner === 0 ? 'Player 1' : (gameState.mode === 'bot' ? 'Bot' : 'Player 2');
        turnInfo.textContent = winnerName + ' wins!';
    } else if (gameState.mode === 'bot' && gameState.current_player === 1) {
        turnInfo.textContent = "Bot is thinking...";
    } else {
        const playerName = gameState.current_player === 0 ? 'P1' : (gameState.mode === 'bot' ? 'Bot' : 'P2');
        turnInfo.textContent = playerName + ' — Combine, place, or end turn';
    }
}

// --- Interactions ---

function toggleHandCard(index) {
    if (gameState.phase === 'game_over') return;
    const player = gameState.players[gameState.current_player];
    const card = player.hand[index];

    if (selectedHandIndices.has(index)) {
        selectedHandIndices.delete(index);
    } else {
        // Enforce max 1 intent
        if (card.kind === 'intent') {
            const hasIntent = Array.from(selectedHandIndices).some(i =>
                player.hand[i].kind === 'intent'
            );
            if (hasIntent) return;
        }
        if (selectedHandIndices.size >= 4) return;
        selectedHandIndices.add(index);
    }
    renderHand();
}

function updateButtons() {
    const player = gameState.players[gameState.current_player];
    const gameOver = gameState.phase === 'game_over';
    const count = selectedHandIndices.size;

    // Combine: need 2+ cards selected, at least 1 material-like
    const materialLikeCount = Array.from(selectedHandIndices).filter(i =>
        player.hand[i].kind === 'material' || player.hand[i].kind === 'crafted'
    ).length;
    document.getElementById('combine-btn').disabled = count < 2 || materialLikeCount < 1 || gameOver;

    // Discard: 1-3 cards selected
    document.getElementById('discard-btn').disabled = count < 1 || count > 3 || gameOver;
}

async function doCombine() {
    if (selectedHandIndices.size < 2) return;
    try {
        showLoading('Alchemizing...');
        const indices = Array.from(selectedHandIndices).sort((a, b) => a - b);
        const result = await api('POST', `/api/game/${gameId}/combine`, {
            card_indices: indices,
            async_image: true,
        });
        gameState = result.game;
        selectedHandIndices.clear();
        render();
        hideLoading();

        if (result.image_pending && result.crafted_card) {
            // New card! Show pre-popup immediately, generate image in background
            await showCardPending(result.crafted_card, result.cache_key);
        } else if (result.is_new && result.crafted_card) {
            await showCardReveal(result.crafted_card, { canClaim: true, cardId: result.cache_key || result.crafted_card.id });
        }
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Fizzled!</h2>
            <p>The combination didn't work.</p>
            <button onclick="hideOverlay()">Try Again</button>
        `);
    }
}

function showCardPending(card, cacheKey) {
    return new Promise(async (resolve) => {
        const overlay = document.createElement('div');
        overlay.className = 'card-reveal-overlay';
        overlay.innerHTML = `
            <div class="reveal-title">New Discovery</div>
            <div class="card-reveal card-pending">
                <div class="pending-spinner"></div>
            </div>
            <div class="reveal-name">${card.name}</div>
            <div class="reveal-desc">${card.description}</div>
            <div class="reveal-status">Generating image...</div>
        `;
        document.body.appendChild(overlay);

        try {
            const result = await api('POST', `/api/game/${gameId}/finalize-combine`, {
                cache_key: cacheKey,
                name: card.name,
                description: card.description,
            });
            gameState = result.game;

            // Replace spinner with actual image
            const cardDiv = overlay.querySelector('.card-reveal');
            cardDiv.classList.remove('card-pending');
            cardDiv.innerHTML = `<img src="${result.image_path}" alt="${card.name}">`;

            // Add claim button if wallet connected
            if (walletPublicKey) {
                const claimBtn = document.createElement('button');
                claimBtn.className = 'claim-nft-btn';
                claimBtn.textContent = 'Claim as NFT';
                claimBtn.onclick = async (e) => {
                    e.stopPropagation();
                    overlay.remove();
                    render();
                    await claimCard(cacheKey, gameId);
                    resolve();
                };
                overlay.insertBefore(claimBtn, overlay.querySelector('.reveal-status'));
            }

            const status = overlay.querySelector('.reveal-status');
            status.textContent = 'Click anywhere to continue';
            status.className = 'reveal-dismiss';

            overlay.onclick = (e) => {
                if (e.target.classList.contains('claim-nft-btn')) return;
                overlay.remove();
                render();
                resolve();
            };
        } catch (e) {
            const status = overlay.querySelector('.reveal-status');
            status.textContent = 'Image failed. Click to continue.';
            status.className = 'reveal-dismiss';
            overlay.onclick = () => {
                overlay.remove();
                render();
                resolve();
            };
        }
    });
}

async function doDiscard() {
    const count = selectedHandIndices.size;
    if (count < 1 || count > 3) return;
    try {
        const indices = Array.from(selectedHandIndices).sort((a, b) => a - b);
        gameState = await api('POST', `/api/game/${gameId}/discard`, {
            card_indices: indices,
        });
        selectedHandIndices.clear();
        render();
    } catch (e) {
        showOverlay(`
            <h2>Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

async function placeCard(row, col) {
    if (gameState.phase === 'game_over') return;
    if (gameState.has_placed) return;

    // Find the selected crafted card, or auto-select if only one
    let handIndex = null;
    const player = gameState.players[gameState.current_player];

    // Check if a crafted card is selected
    for (const idx of selectedHandIndices) {
        if (player.hand[idx].kind === 'crafted') {
            handIndex = idx;
            break;
        }
    }

    // Auto-select if exactly one crafted card in hand and none selected
    if (handIndex === null) {
        const craftedIndices = player.hand
            .map((c, i) => c.kind === 'crafted' ? i : -1)
            .filter(i => i >= 0);
        if (craftedIndices.length === 1) {
            handIndex = craftedIndices[0];
        } else if (craftedIndices.length === 0) {
            return;
        } else {
            showOverlay(`
                <h2>No Card Selected</h2>
                <p>Select a crafted card from your hand first.</p>
                <button onclick="hideOverlay()">Dismiss</button>
            `);
            return;
        }
    }

    try {
        showLoading('Placing card...');
        const result = await api('POST', `/api/game/${gameId}/place`, {
            hand_index: handIndex,
            row,
            col,
        });

        gameState = result.game;
        selectedHandIndices.clear();

        if (result.judgment) {
            const j = result.judgment;
            const won = result.result === 'conquered';
            hideLoading();
            showOverlay(`
                <h2>${won ? 'Conquest!' : 'Defended!'}</h2>
                <p><strong>${j.attacker}</strong> vs <strong>${j.defender}</strong></p>
                <p>Category: <strong>${j.category}</strong></p>
                <p>${j.reason}</p>
                <p>${won ? 'The attacker takes the cell!' : 'The defender holds! Your card is returned to your hand.'}</p>
                <button onclick="afterPlace()">Continue</button>
            `);
        } else {
            hideLoading();
            afterPlace();
        }
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

function afterPlace() {
    hideOverlay();
    render();

    if (gameState.phase === 'game_over') {
        showWinScreen();
    }
}

async function endTurn() {
    if (gameState.phase === 'game_over') return;
    try {
        showLoading('Ending turn...');
        gameState = await api('POST', `/api/game/${gameId}/end-turn`);
        selectedHandIndices.clear();
        render();
        hideLoading();

        if (gameState.phase === 'game_over') {
            showWinScreen();
            return;
        }

        // Trigger bot turn if needed
        if (gameState.mode === 'bot' && gameState.current_player === 1) {
            await doBotTurn();
        }
    } catch (e) {
        hideLoading();
        showOverlay(`
            <h2>Failed</h2>
            <p>${e.message}</p>
            <button onclick="hideOverlay()">Dismiss</button>
        `);
    }
}

async function doBotTurn() {
    // Phase 1: Bot combines
    try {
        showLoading('Bot is crafting...');
        const combineResult = await api('POST', `/api/game/${gameId}/bot-combine`);
        gameState = combineResult.game;
        render();

        if (combineResult.result === 'bot_failed') {
            hideLoading();
            render();
            return;
        }

        // Show the crafted card if it's new
        if (combineResult.is_new && combineResult.crafted_card) {
            hideLoading();
            await showCardReveal(combineResult.crafted_card);
        }
    } catch (e) {
        hideLoading();
        try { gameState = await api('GET', `/api/game/${gameId}`); } catch (_) {}
        render();
        return;
    }

    // Phase 2: Bot places
    try {
        showLoading('Bot is placing...');
        const placeResult = await api('POST', `/api/game/${gameId}/bot-place`);
        gameState = placeResult.game;
        hideLoading();

        if (placeResult.judgment) {
            const j = placeResult.judgment;
            const won = placeResult.result === 'conquered';
            render();
            showOverlay(`
                <h2>Bot ${won ? 'Conquered!' : 'Failed to Conquer'}</h2>
                <p><strong>${j.attacker}</strong> vs <strong>${j.defender}</strong></p>
                <p>Category: <strong>${j.category}</strong></p>
                <p>${j.reason}</p>
                <button onclick="afterBotTurn()">Continue</button>
            `);
        } else {
            afterBotTurn();
        }
    } catch (e) {
        hideLoading();
        try { gameState = await api('GET', `/api/game/${gameId}`); } catch (_) {}
        render();
    }
}

function afterBotTurn() {
    hideOverlay();
    render();
    if (gameState.phase === 'game_over') {
        showWinScreen();
    }
}

function showWinScreen() {
    const winnerName = gameState.winner === 0 ? 'Player 1' : (gameState.mode === 'bot' ? 'Bot' : 'Player 2');
    showOverlay(`
        <h2 class="win-title">${winnerName} Wins!</h2>
        <p>Score: ${gameState.players[0].score} - ${gameState.players[1].score}</p>
        <button onclick="backToMenu()">Back to Menu</button>
    `);
}

function confirmQuit() {
    showOverlay(`
        <h2>Quit Game?</h2>
        <p>Are you sure you want to leave this game?</p>
        <button onclick="backToMenu()">Quit</button>
        <button onclick="hideOverlay()" style="margin-left:10px;border-color:#3a3040;color:#6a5a40">Cancel</button>
    `);
}

function backToMenu() {
    hideOverlay();
    gameId = null;
    gameState = null;
    selectedHandIndices.clear();
    showScreen('title-screen');
}
