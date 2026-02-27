use crate::combine::CombineResult;
use crate::theories::{BOARD_CATEGORIES, TARGET_ITEMS};
use std::collections::{HashMap, HashSet};
use std::io::Write;

/// Tracks all results for reporting.
pub struct Report {
    /// Step 1 results: family_name -> [(pair_label, modifier_name, result)]
    pub modifier_results: HashMap<String, Vec<(String, String, CombineResult)>>,
    /// Step 1 bare-pair results (no modifier): pair_label -> result
    pub bare_results: HashMap<String, CombineResult>,
    /// Step 2 results: theory_name -> [(pair_label, result)]
    pub theory_results: HashMap<String, Vec<(String, CombineResult)>>,
    /// Step 2 results with modifier: theory_name -> [(pair_label, result)]
    pub theory_modifier_results: HashMap<String, Vec<(String, CombineResult)>>,
    /// Second-order results: label -> result
    pub second_order_results: Vec<(String, CombineResult)>,
    /// Third-order results: label -> result
    pub third_order_results: Vec<(String, CombineResult)>,
    /// Category scores: card_name -> { category -> score }
    pub category_scores: HashMap<String, HashMap<String, u32>>,
    /// Winning modifier family from step 1
    pub winning_family: Option<String>,
    /// Winning theory from step 2
    pub winning_theory: Option<String>,
}

impl Report {
    pub fn new() -> Self {
        Self {
            modifier_results: HashMap::new(),
            bare_results: HashMap::new(),
            theory_results: HashMap::new(),
            theory_modifier_results: HashMap::new(),
            second_order_results: Vec::new(),
            third_order_results: Vec::new(),
            category_scores: HashMap::new(),
            winning_family: None,
            winning_theory: None,
        }
    }

    /// Compute modifier family metrics and print comparison.
    pub fn print_modifier_comparison(&mut self) {
        println!("\n{}", "=".repeat(60));
        println!("STEP 1: MODIFIER FAMILY COMPARISON");
        println!("{}\n", "=".repeat(60));

        let mut family_scores: Vec<(&str, usize, usize, f64)> = Vec::new();

        // Print bare results first
        println!("--- Bare pairs (no modifier) ---");
        let mut bare_valid = 0;
        let mut bare_unique = HashSet::new();
        for (pair, result) in &self.bare_results {
            let valid = result.name != "Not possible";
            if valid {
                bare_valid += 1;
                bare_unique.insert(result.name.clone());
            }
            let marker = if valid { "+" } else { "-" };
            println!("  [{marker}] {pair} = {} — {}", result.name, result.description);
        }
        let bare_total = self.bare_results.len();
        println!(
            "  Valid: {bare_valid}/{bare_total}, Unique: {}\n",
            bare_unique.len()
        );

        for (family_name, results) in &self.modifier_results {
            println!("--- Family: {family_name} ---");

            let mut valid = 0;
            let mut unique_names = HashSet::new();
            let mut differentiation_groups: HashMap<String, HashSet<String>> = HashMap::new();

            for (pair, modifier, result) in results {
                let is_valid = result.name != "Not possible";
                if is_valid {
                    valid += 1;
                    unique_names.insert(result.name.clone());
                }
                differentiation_groups
                    .entry(pair.clone())
                    .or_default()
                    .insert(result.name.clone());

                let marker = if is_valid { "+" } else { "-" };
                println!(
                    "  [{marker}] {pair} + [{modifier}] = {} — {}",
                    result.name, result.description
                );
            }

            let total = results.len();
            let diff_score: f64 = if differentiation_groups.is_empty() {
                0.0
            } else {
                differentiation_groups
                    .values()
                    .map(|names| names.len() as f64)
                    .sum::<f64>()
                    / differentiation_groups.len() as f64
            };

            println!(
                "  Valid: {valid}/{total}, Unique: {}, Avg differentiation: {diff_score:.1}\n",
                unique_names.len()
            );

            family_scores.push((
                // Leaking is fine — these are static-lifetime strings in practice
                Box::leak(family_name.clone().into_boxed_str()),
                valid,
                unique_names.len(),
                diff_score,
            ));
        }

        // Pick winner: highest (valid + unique + differentiation)
        family_scores.sort_by(|a, b| {
            let score_a = a.1 as f64 + a.2 as f64 + a.3;
            let score_b = b.1 as f64 + b.2 as f64 + b.3;
            score_b.partial_cmp(&score_a).unwrap()
        });

        println!("MODIFIER RANKING:");
        for (i, (name, valid, unique, diff)) in family_scores.iter().enumerate() {
            let marker = if i == 0 { " <-- WINNER" } else { "" };
            println!(
                "  {}. {name}: valid={valid}, unique={unique}, diff={diff:.1}{marker}",
                i + 1
            );
        }

        if let Some((winner, _, _, _)) = family_scores.first() {
            self.winning_family = Some(winner.to_string());
        }
    }

    /// Compute element theory metrics and print comparison.
    pub fn print_theory_comparison(&mut self) {
        println!("\n{}", "=".repeat(60));
        println!("STEP 2: ELEMENT SET COMPARISON");
        println!("{}\n", "=".repeat(60));

        let mut theory_scores: Vec<(String, usize, usize, usize, usize)> = Vec::new();

        for (theory_name, results) in &self.theory_results {
            let mod_results = self.theory_modifier_results.get(theory_name);

            println!("--- Theory: {theory_name} ---");

            let mut valid = 0;
            let mut unique_names = HashSet::new();

            for (pair, result) in results {
                let is_valid = result.name != "Not possible";
                if is_valid {
                    valid += 1;
                    unique_names.insert(result.name.clone());
                }
                let marker = if is_valid { "+" } else { "-" };
                println!(
                    "  [{marker}] {pair} = {} — {}",
                    result.name, result.description
                );
            }

            let total = results.len();
            let mut mod_valid = 0;
            let mut mod_unique = HashSet::new();

            if let Some(mr) = mod_results {
                for (pair, result) in mr {
                    let is_valid = result.name != "Not possible";
                    if is_valid {
                        mod_valid += 1;
                        mod_unique.insert(result.name.clone());
                        unique_names.insert(result.name.clone());
                    }
                    let marker = if is_valid { "+" } else { "-" };
                    println!(
                        "  [{marker}] {pair} (w/ modifier) = {} — {}",
                        result.name, result.description
                    );
                }
            }

            println!(
                "  Bare: {valid}/{total} valid, With modifier: {mod_valid} valid, Total unique: {}\n",
                unique_names.len()
            );

            let target_found = count_target_items(&unique_names);
            theory_scores.push((
                theory_name.clone(),
                valid,
                unique_names.len(),
                target_found,
                mod_valid,
            ));
        }

        // Sort by (valid + unique + target_found)
        theory_scores.sort_by(|a, b| {
            let score_a = a.1 + a.2 + a.3 * 3 + a.4;
            let score_b = b.1 + b.2 + b.3 * 3 + b.4;
            score_b.cmp(&score_a)
        });

        println!("THEORY RANKING:");
        for (i, (name, valid, unique, targets, mod_valid)) in theory_scores.iter().enumerate() {
            let marker = if i == 0 { " <-- WINNER" } else { "" };
            println!(
                "  {}. {name}: valid={valid}, unique={unique}, targets={targets}, mod_valid={mod_valid}{marker}",
                i + 1
            );
        }

        if let Some((winner, _, _, _, _)) = theory_scores.first() {
            self.winning_theory = Some(winner.clone());
        }
    }

    /// Print second and third order chain results.
    pub fn print_deep_results(&self) {
        if !self.second_order_results.is_empty() {
            println!("\n{}", "=".repeat(60));
            println!("STEP 3: SECOND-ORDER CHAINS");
            println!("{}\n", "=".repeat(60));

            let mut valid = 0;
            let mut unique = HashSet::new();
            for (label, result) in &self.second_order_results {
                let is_valid = result.name != "Not possible";
                if is_valid {
                    valid += 1;
                    unique.insert(result.name.clone());
                }
                let marker = if is_valid { "+" } else { "-" };
                println!("  [{marker}] {label} = {} — {}", result.name, result.description);
            }
            println!(
                "\n  Valid: {valid}/{}, Unique: {}",
                self.second_order_results.len(),
                unique.len()
            );
        }

        if !self.third_order_results.is_empty() {
            println!("\n--- Third-order chains ---");
            let mut valid = 0;
            let mut unique = HashSet::new();
            for (label, result) in &self.third_order_results {
                let is_valid = result.name != "Not possible";
                if is_valid {
                    valid += 1;
                    unique.insert(result.name.clone());
                }
                let marker = if is_valid { "+" } else { "-" };
                println!("  [{marker}] {label} = {} — {}", result.name, result.description);
            }
            println!(
                "\n  Valid: {valid}/{}, Unique: {}",
                self.third_order_results.len(),
                unique.len()
            );
        }
    }

    /// Print target items checklist.
    pub fn print_target_checklist(&self) {
        println!("\n{}", "=".repeat(60));
        println!("TARGET ITEMS CHECKLIST");
        println!("{}\n", "=".repeat(60));

        let all_names: HashSet<String> = self.all_result_names();

        for (category, items) in TARGET_ITEMS {
            println!("  {category}:");
            for item in *items {
                let found = all_names.iter().any(|n| {
                    n.eq_ignore_ascii_case(item) || n.to_lowercase().contains(&item.to_lowercase())
                });
                let check = if found { "x" } else { " " };
                println!("    [{check}] {item}");
            }
        }
    }

    /// Print category coverage summary.
    pub fn print_category_coverage(&self) {
        if self.category_scores.is_empty() {
            return;
        }

        println!("\n{}", "=".repeat(60));
        println!("CATEGORY COVERAGE");
        println!("{}\n", "=".repeat(60));

        // For each category, find the best-scoring card
        for cat in BOARD_CATEGORIES {
            let mut best: Option<(&str, u32)> = None;
            for (card_name, scores) in &self.category_scores {
                if let Some(&score) = scores.get(*cat) {
                    if best.is_none() || score > best.unwrap().1 {
                        best = Some((card_name, score));
                    }
                }
            }
            match best {
                Some((name, score)) => {
                    let bar = "#".repeat(score as usize);
                    println!("  {cat:.<25} {score:>2}/10 {bar} ({name})");
                }
                None => println!("  {cat:.<25}  --"),
            }
        }
    }

    /// Write full report to file.
    pub fn write_to_file(&self, path: &str) {
        let mut f = std::fs::File::create(path).expect("failed to create report file");

        writeln!(f, "# Explore Report\n").unwrap();

        if let Some(ref family) = self.winning_family {
            writeln!(f, "**Winning modifier family:** {family}").unwrap();
        }
        if let Some(ref theory) = self.winning_theory {
            writeln!(f, "**Winning element theory:** {theory}").unwrap();
        }

        // Target checklist
        writeln!(f, "\n## Target Items\n").unwrap();
        let all_names = self.all_result_names();
        for (category, items) in TARGET_ITEMS {
            writeln!(f, "### {category}").unwrap();
            for item in *items {
                let found = all_names.iter().any(|n| {
                    n.eq_ignore_ascii_case(item) || n.to_lowercase().contains(&item.to_lowercase())
                });
                let check = if found { "x" } else { " " };
                writeln!(f, "- [{check}] {item}").unwrap();
            }
        }

        // Category coverage
        if !self.category_scores.is_empty() {
            writeln!(f, "\n## Category Coverage\n").unwrap();
            writeln!(f, "| Category | Best Score | Best Card |").unwrap();
            writeln!(f, "|----------|-----------|-----------|").unwrap();
            for cat in BOARD_CATEGORIES {
                let mut best: Option<(&str, u32)> = None;
                for (card_name, scores) in &self.category_scores {
                    if let Some(&score) = scores.get(*cat) {
                        if best.is_none() || score > best.unwrap().1 {
                            best = Some((card_name, score));
                        }
                    }
                }
                match best {
                    Some((name, score)) => writeln!(f, "| {cat} | {score}/10 | {name} |").unwrap(),
                    None => writeln!(f, "| {cat} | -- | -- |").unwrap(),
                }
            }
        }

        println!("\nReport written to {path}");
    }

    /// Returns all valid result (name, description) pairs for scoring.
    pub fn all_result_names_with_desc(&self) -> Vec<(String, String)> {
        let mut results = Vec::new();
        let mut add = |r: &CombineResult| {
            if r.name != "Not possible" {
                results.push((r.name.clone(), r.description.clone()));
            }
        };
        for result in self.bare_results.values() {
            add(result);
        }
        for family_results in self.modifier_results.values() {
            for (_, _, result) in family_results {
                add(result);
            }
        }
        for theory_results in self.theory_results.values() {
            for (_, result) in theory_results {
                add(result);
            }
        }
        for theory_results in self.theory_modifier_results.values() {
            for (_, result) in theory_results {
                add(result);
            }
        }
        for (_, result) in &self.second_order_results {
            add(result);
        }
        for (_, result) in &self.third_order_results {
            add(result);
        }
        results
    }

    fn all_result_names(&self) -> HashSet<String> {
        let mut names = HashSet::new();
        for result in self.bare_results.values() {
            if result.name != "Not possible" {
                names.insert(result.name.clone());
            }
        }
        for results in self.modifier_results.values() {
            for (_, _, result) in results {
                if result.name != "Not possible" {
                    names.insert(result.name.clone());
                }
            }
        }
        for results in self.theory_results.values() {
            for (_, result) in results {
                if result.name != "Not possible" {
                    names.insert(result.name.clone());
                }
            }
        }
        for results in self.theory_modifier_results.values() {
            for (_, result) in results {
                if result.name != "Not possible" {
                    names.insert(result.name.clone());
                }
            }
        }
        for (_, result) in &self.second_order_results {
            if result.name != "Not possible" {
                names.insert(result.name.clone());
            }
        }
        for (_, result) in &self.third_order_results {
            if result.name != "Not possible" {
                names.insert(result.name.clone());
            }
        }
        names
    }
}

fn count_target_items(names: &HashSet<String>) -> usize {
    let mut count = 0;
    for (_, items) in TARGET_ITEMS {
        for item in *items {
            if names.iter().any(|n| {
                n.eq_ignore_ascii_case(item) || n.to_lowercase().contains(&item.to_lowercase())
            }) {
                count += 1;
            }
        }
    }
    count
}
