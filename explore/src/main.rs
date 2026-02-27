mod cache;
mod combine;
mod report;
mod theories;

use cache::Cache;
use clap::Parser;
use combine::OllamaClient;
use report::Report;
use std::path::PathBuf;
use theories::{
    all_modifier_families, all_theories, baseline_elements, sample_pairs, sensory_variations,
    theory_g_elements, Card, BOARD_CATEGORIES,
};

#[derive(Parser)]
#[command(name = "explore", about = "Explore element and modifier combinations")]
struct Cli {
    /// Run only a specific step (1 = modifiers, 2 = elements)
    #[arg(long)]
    step: Option<u32>,

    /// Include second-order and third-order chain tests
    #[arg(long)]
    deep: bool,

    /// Test Sensory modifier variations against Theory G
    #[arg(long)]
    sensory: bool,

    /// Skip category scoring
    #[arg(long)]
    no_score: bool,

    /// Ollama base URL
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Ollama model name
    #[arg(long, default_value = "gemma3:4b")]
    model: String,
}

struct Stats {
    calls: usize,
    valid: usize,
    cached: usize,
}

impl Stats {
    fn new() -> Self {
        Self {
            calls: 0,
            valid: 0,
            cached: 0,
        }
    }

    fn print_running(&self) {
        let valid_pct = if self.calls > 0 {
            self.valid as f64 / self.calls as f64 * 100.0
        } else {
            0.0
        };
        eprint!(
            "\r  [{} calls, {:.0}% valid, {} cached]",
            self.calls, valid_pct, self.cached
        );
    }
}

async fn do_combine(
    client: &OllamaClient,
    cache: &mut Cache,
    cache_path: &PathBuf,
    cards: &[Card],
    label: &str,
    stats: &mut Stats,
) -> combine::CombineResult {
    stats.calls += 1;

    // Check cache
    if let Some(cached) = cache.get(cards) {
        stats.cached += 1;
        let valid = cached.name != "Not possible";
        if valid {
            stats.valid += 1;
        }
        let marker = if valid { "+" } else { "-" };
        println!("  [{marker}] {label} = {} (cached)", cached.name);
        stats.print_running();
        return cached;
    }

    match client.combine(cards).await {
        Ok(result) => {
            let valid = result.name != "Not possible";
            if valid {
                stats.valid += 1;
            }
            let marker = if valid { "+" } else { "-" };
            println!(
                "  [{marker}] {label} = {} — {}",
                result.name, result.description
            );
            cache.insert(cards, &result);
            cache.save(cache_path);
            stats.print_running();
            result
        }
        Err(e) => {
            eprintln!("  [!] {label} ERROR: {e}");
            stats.print_running();
            combine::CombineResult {
                name: "Not possible".to_string(),
                description: format!("Error: {e}"),
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let client = OllamaClient::new(&cli.ollama_url, &cli.model);
    let cache_path = PathBuf::from("explore/cache.json");
    let mut cache = Cache::load(&cache_path);
    let mut report = Report::new();
    let mut stats = Stats::new();

    println!("Explore: Ollama at {}, model {}", cli.ollama_url, cli.model);
    println!("Cache: {} entries loaded\n", cache.len());

    // ========== Sensory variations mode ==========
    if cli.sensory {
        println!("=== SENSORY MODIFIER VARIATIONS (Theory G) ===\n");

        let elements = theory_g_elements();
        let pairs = sample_pairs(&elements);
        let variations = sensory_variations();

        // Bare pairs first
        println!("--- Bare pairs (Theory G, no modifier) ---");
        for (a, b) in &pairs {
            let label = format!("{} + {}", a.name, b.name);
            let cards = vec![a.clone(), b.clone()];
            let result =
                do_combine(&client, &mut cache, &cache_path, &cards, &label, &mut stats).await;
            report.bare_results.insert(label, result);
        }
        println!();

        // Test each Sensory variation
        for family in &variations {
            println!("--- {} ---", family.name);

            let mut family_results = Vec::new();

            for (a, b) in &pairs {
                for modifier in &family.modifiers {
                    let label = format!(
                        "{} + {} [{}]",
                        a.name, b.name, modifier.name
                    );
                    let cards = vec![a.clone(), b.clone(), modifier.clone()];
                    let result = do_combine(
                        &client,
                        &mut cache,
                        &cache_path,
                        &cards,
                        &label,
                        &mut stats,
                    )
                    .await;
                    family_results.push((
                        format!("{} + {}", a.name, b.name),
                        modifier.name.clone(),
                        result,
                    ));
                }
            }
            println!();

            report
                .modifier_results
                .insert(family.name.to_string(), family_results);
        }

        report.print_modifier_comparison();
        report.print_target_checklist();
        report.write_to_file("explore/report.md");

        println!(
            "\nDone! {} total calls ({} cached), {:.0}% valid",
            stats.calls,
            stats.cached,
            if stats.calls > 0 {
                stats.valid as f64 / stats.calls as f64 * 100.0
            } else {
                0.0
            }
        );
        return;
    }

    let run_step1 = cli.step.is_none() || cli.step == Some(1);
    let run_step2 = cli.step.is_none() || cli.step == Some(2);

    // ========== STEP 1: Modifier family comparison ==========
    if run_step1 {
        println!("=== STEP 1: Modifier Family Comparison ===\n");

        let elements = baseline_elements();
        let pairs = sample_pairs(&elements);
        let families = all_modifier_families();

        // Bare pairs (no modifier)
        println!("--- Bare pairs ---");
        for (a, b) in &pairs {
            let label = format!("{} + {}", a.name, b.name);
            let cards = vec![a.clone(), b.clone()];
            let result =
                do_combine(&client, &mut cache, &cache_path, &cards, &label, &mut stats).await;
            report.bare_results.insert(label, result);
        }
        println!();

        // Each modifier family
        for family in &families {
            println!("--- Family: {} ---", family.name);

            let mut family_results = Vec::new();

            for (a, b) in &pairs {
                for modifier in &family.modifiers {
                    let label = format!(
                        "{} + {} [{}]",
                        a.name, b.name, modifier.name
                    );
                    let cards = vec![a.clone(), b.clone(), modifier.clone()];
                    let result = do_combine(
                        &client,
                        &mut cache,
                        &cache_path,
                        &cards,
                        &label,
                        &mut stats,
                    )
                    .await;
                    family_results.push((
                        format!("{} + {}", a.name, b.name),
                        modifier.name.clone(),
                        result,
                    ));
                }
            }
            println!();

            report
                .modifier_results
                .insert(family.name.to_string(), family_results);
        }

        report.print_modifier_comparison();
    }

    // ========== STEP 2: Element set comparison ==========
    if run_step2 {
        println!("\n=== STEP 2: Element Set Comparison ===\n");

        // Determine best modifier to use (from step 1 or cache)
        let winning_family_name = report
            .winning_family
            .clone()
            .unwrap_or_else(|| "Evocative".to_string());

        let families = all_modifier_families();
        let winning_family = families
            .iter()
            .find(|f| f.name == winning_family_name)
            .expect("winning family not found");

        // Pick a representative modifier from the winning family (first one)
        let best_modifier = &winning_family.modifiers[0];
        println!(
            "Using modifier family '{}', representative modifier '{}'\n",
            winning_family_name, best_modifier.name
        );

        for theory in all_theories() {
            println!("--- Theory {}: {} ---", theory.name, theory.label);

            let n = theory.elements.len();
            let mut bare_results = Vec::new();
            let mut mod_results = Vec::new();

            for i in 0..n {
                for j in (i + 1)..n {
                    let a = &theory.elements[i];
                    let b = &theory.elements[j];

                    // Bare combination
                    let label = format!("{} + {}", a.name, b.name);
                    let cards = vec![a.clone(), b.clone()];
                    let result = do_combine(
                        &client,
                        &mut cache,
                        &cache_path,
                        &cards,
                        &label,
                        &mut stats,
                    )
                    .await;
                    bare_results.push((label, result));

                    // With modifier
                    let label = format!(
                        "{} + {} [{}]",
                        a.name, b.name, best_modifier.name
                    );
                    let cards = vec![a.clone(), b.clone(), best_modifier.clone()];
                    let result = do_combine(
                        &client,
                        &mut cache,
                        &cache_path,
                        &cards,
                        &label,
                        &mut stats,
                    )
                    .await;
                    mod_results.push((label, result));
                }
            }
            println!();

            let key = format!("{}: {}", theory.name, theory.label);
            report.theory_results.insert(key.clone(), bare_results);
            report.theory_modifier_results.insert(key, mod_results);
        }

        report.print_theory_comparison();
    }

    // ========== STEP 3: Deep chains ==========
    if cli.deep {
        println!("\n=== STEP 3: Second-Order + Third-Order Chains ===\n");

        // Collect top first-order results (valid, from winning theory or all)
        let all_valid: Vec<(String, combine::CombineResult)> = report
            .theory_results
            .values()
            .flatten()
            .chain(report.theory_modifier_results.values().flatten())
            .chain(report.bare_results.iter().map(|(k, v)| {
                // Convert &(String, CombineResult) references
                (k.clone(), v.clone())
            }).collect::<Vec<_>>().iter())
            .filter(|(_, r)| r.name != "Not possible")
            .cloned()
            .collect();

        // Deduplicate by name, take top 15
        let mut seen = std::collections::HashSet::new();
        let top_first_order: Vec<combine::CombineResult> = all_valid
            .iter()
            .filter(|(_, r)| seen.insert(r.name.clone()))
            .map(|(_, r)| r.clone())
            .take(15)
            .collect();

        println!(
            "Using {} unique first-order results for second-order chains\n",
            top_first_order.len()
        );

        // Get base elements from winning theory (or default to Classical)
        let theories = all_theories();
        let winning_theory_name = report
            .winning_theory
            .as_deref()
            .unwrap_or("A: Classical");
        let base_elements: &[Card] = &theories
            .iter()
            .find(|t| {
                let key = format!("{}: {}", t.name, t.label);
                key == winning_theory_name
            })
            .unwrap_or(&theories[0])
            .elements;

        // Second-order: each first-order result + each base element
        println!("--- Second-order ---");
        for first_result in &top_first_order {
            let result_card = Card::material(&first_result.name, &first_result.description);

            for base in base_elements {
                let label = format!("{} + {}", first_result.name, base.name);
                let cards = vec![result_card.clone(), base.clone()];
                let result = do_combine(
                    &client,
                    &mut cache,
                    &cache_path,
                    &cards,
                    &label,
                    &mut stats,
                )
                .await;
                report.second_order_results.push((label, result));
            }
        }
        println!();

        // Collect top second-order for third-order
        let mut seen2 = std::collections::HashSet::new();
        let top_second_order: Vec<combine::CombineResult> = report
            .second_order_results
            .iter()
            .filter(|(_, r)| r.name != "Not possible" && seen2.insert(r.name.clone()))
            .map(|(_, r)| r.clone())
            .take(10)
            .collect();

        // Third-order: top second-order × top first-order
        if !top_second_order.is_empty() {
            println!("--- Third-order ---");
            let first_top10: Vec<_> = top_first_order.iter().take(10).collect();
            for second in &top_second_order {
                let s_card = Card::material(&second.name, &second.description);
                for first in &first_top10 {
                    let f_card = Card::material(&first.name, &first.description);
                    let label = format!("{} + {}", second.name, first.name);
                    let cards = vec![s_card.clone(), f_card.clone()];
                    let result = do_combine(
                        &client,
                        &mut cache,
                        &cache_path,
                        &cards,
                        &label,
                        &mut stats,
                    )
                    .await;
                    report.third_order_results.push((label, result));
                }
            }
            println!();
        }

        report.print_deep_results();
    }

    // ========== STEP 4: Category scoring ==========
    if !cli.no_score {
        println!("\n=== STEP 4: Category Scoring ===\n");

        let all_names = report.all_result_names_with_desc();

        // Limit to unique valid results
        let mut scored = std::collections::HashSet::new();
        let to_score: Vec<(String, String)> = all_names
            .into_iter()
            .filter(|(name, _)| scored.insert(name.clone()))
            .collect();

        println!("Scoring {} unique cards against {} categories...\n", to_score.len(), BOARD_CATEGORIES.len());

        for (name, desc) in &to_score {
            eprint!("  Scoring {name}...");
            match client
                .score_categories(name, desc, BOARD_CATEGORIES)
                .await
            {
                Ok(scores) => {
                    let top_cat = scores
                        .iter()
                        .max_by_key(|(_, &v)| v)
                        .map(|(k, v)| format!("{k}={v}"))
                        .unwrap_or_default();
                    eprintln!(" done (best: {top_cat})");
                    report.category_scores.insert(name.clone(), scores);
                }
                Err(e) => {
                    eprintln!(" error: {e}");
                }
            }
        }

        report.print_category_coverage();
    }

    // ========== Final output ==========
    report.print_target_checklist();
    report.write_to_file("explore/report.md");

    println!(
        "\nDone! {} total calls ({} cached), {:.0}% valid",
        stats.calls,
        stats.cached,
        if stats.calls > 0 {
            stats.valid as f64 / stats.calls as f64 * 100.0
        } else {
            0.0
        }
    );
}
