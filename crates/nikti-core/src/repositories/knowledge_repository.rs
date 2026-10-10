use crate::types::common::{AnalysisRulesetId, RecipeId, ValidationRulesetId};
use crate::types::{AnalysisRuleset, Recipe, ValidationRuleset};

use super::errors::RepositoryError;

/// INV-006/009: stores and manages Recipes and Rulesets, but never authors
/// them — authoring happens in the Project Workspace (Recipes) or the Table
/// Repository's Ruleset Editor (Rulesets), never here. Unlike Table/Report
/// repositories, update methods are legitimate: Knowledge is editable.
pub trait KnowledgeRepository {
    async fn insert_recipe(&self, recipe: Recipe) -> Result<(), RepositoryError>;
    async fn get_recipe(&self, id: RecipeId) -> Result<Option<Recipe>, RepositoryError>;
    async fn list_recipes(&self) -> Result<Vec<Recipe>, RepositoryError>;
    async fn update_recipe(&self, recipe: Recipe) -> Result<(), RepositoryError>;

    /// A.5: the Knowledge Repository's entire Ruleset surface is
    /// store/get/list — no create/edit entry point. `store_validation_ruleset`
    /// takes a whole, already-authored-and-versioned `ValidationRuleset`
    /// (an upsert on `ruleset_id`) rather than an `insert`/`update` pair,
    /// deliberately: a named `insert` vs `update` pair invites exactly the
    /// kind of partial-field editing API ("update just the rules list")
    /// that INV-009 forbids happening anywhere but Table Repository
    /// context. Authoring/editing (constructing the new `ValidationRuleset`
    /// or `ValidationRulesetVersion` value in the first place, via
    /// `ValidationRuleset::add_draft_version`/`activate_version`/
    /// `ValidationRulesetVersion::set_rules`) happens only in code paths
    /// reachable from Table Repository context, before this method is
    /// ever called — this repository just persists the resulting value.
    async fn store_validation_ruleset(&self, ruleset: ValidationRuleset) -> Result<(), RepositoryError>;
    async fn get_validation_ruleset(
        &self,
        id: ValidationRulesetId,
    ) -> Result<Option<ValidationRuleset>, RepositoryError>;
    async fn list_validation_rulesets(&self) -> Result<Vec<ValidationRuleset>, RepositoryError>;

    /// B.8: same store/get/list-only surface as
    /// `store_validation_ruleset` above, for the same reason — see that
    /// method's doc comment, not repeated here.
    async fn store_analysis_ruleset(&self, ruleset: AnalysisRuleset) -> Result<(), RepositoryError>;
    async fn get_analysis_ruleset(
        &self,
        id: AnalysisRulesetId,
    ) -> Result<Option<AnalysisRuleset>, RepositoryError>;
    async fn list_analysis_rulesets(&self) -> Result<Vec<AnalysisRuleset>, RepositoryError>;
}

/// Test/dev in-memory reference implementation — same status as
/// `table_repository::testing::InMemoryTableRepository`. Its Recipe
/// methods were `todo!()` until `commands::project_workspace` needed
/// real Recipe promotion/loading to test against; the Ruleset methods
/// predate that and are unchanged.
pub mod testing {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct InMemoryKnowledgeRepository {
        recipes: Mutex<HashMap<RecipeId, Recipe>>,
        validation_rulesets: Mutex<HashMap<ValidationRulesetId, ValidationRuleset>>,
        analysis_rulesets: Mutex<HashMap<AnalysisRulesetId, AnalysisRuleset>>,
    }

    impl InMemoryKnowledgeRepository {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl KnowledgeRepository for InMemoryKnowledgeRepository {
        async fn insert_recipe(&self, recipe: Recipe) -> Result<(), RepositoryError> {
            self.recipes.lock().unwrap().insert(recipe.id(), recipe);
            Ok(())
        }

        async fn get_recipe(&self, id: RecipeId) -> Result<Option<Recipe>, RepositoryError> {
            Ok(self.recipes.lock().unwrap().get(&id).cloned())
        }

        async fn list_recipes(&self) -> Result<Vec<Recipe>, RepositoryError> {
            Ok(self.recipes.lock().unwrap().values().cloned().collect())
        }

        async fn update_recipe(&self, recipe: Recipe) -> Result<(), RepositoryError> {
            let mut recipes = self.recipes.lock().unwrap();
            if !recipes.contains_key(&recipe.id()) {
                return Err(RepositoryError::NotFound);
            }
            recipes.insert(recipe.id(), recipe);
            Ok(())
        }

        async fn store_validation_ruleset(&self, ruleset: ValidationRuleset) -> Result<(), RepositoryError> {
            self.validation_rulesets.lock().unwrap().insert(ruleset.ruleset_id(), ruleset);
            Ok(())
        }

        async fn get_validation_ruleset(&self, id: ValidationRulesetId) -> Result<Option<ValidationRuleset>, RepositoryError> {
            Ok(self.validation_rulesets.lock().unwrap().get(&id).cloned())
        }

        async fn list_validation_rulesets(&self) -> Result<Vec<ValidationRuleset>, RepositoryError> {
            Ok(self.validation_rulesets.lock().unwrap().values().cloned().collect())
        }

        async fn store_analysis_ruleset(&self, ruleset: AnalysisRuleset) -> Result<(), RepositoryError> {
            self.analysis_rulesets.lock().unwrap().insert(ruleset.analysis_ruleset_id(), ruleset);
            Ok(())
        }

        async fn get_analysis_ruleset(&self, id: AnalysisRulesetId) -> Result<Option<AnalysisRuleset>, RepositoryError> {
            Ok(self.analysis_rulesets.lock().unwrap().get(&id).cloned())
        }

        async fn list_analysis_rulesets(&self) -> Result<Vec<AnalysisRuleset>, RepositoryError> {
            Ok(self.analysis_rulesets.lock().unwrap().values().cloned().collect())
        }
    }
}
