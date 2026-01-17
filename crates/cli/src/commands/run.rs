//! Run command - start pipelines

use clap::Subcommand;
use oj_core::pipeline::Pipeline;
use oj_core::storage::JsonStore;
use oj_core::workspace::Workspace;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum RunCommand {
    /// Start a build pipeline
    Build {
        /// Name for this build
        name: String,
        /// Prompt describing what to build
        prompt: String,
    },
    /// Start a bugfix pipeline
    Bugfix {
        /// Issue ID to fix
        id: String,
    },
}

pub async fn handle(command: RunCommand) -> anyhow::Result<()> {
    match command {
        RunCommand::Build { name, prompt } => run_build(name, prompt).await,
        RunCommand::Bugfix { id } => run_bugfix(id).await,
    }
}

async fn run_build(name: String, prompt: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    // Create workspace path
    let workspace_path = PathBuf::from(format!(".worktrees/build-{}", name));
    let branch = format!("build-{}", name);

    // Create workspace state
    let workspace_id = format!("build-{}", name);
    let workspace = Workspace::new(&workspace_id, &name, workspace_path.clone(), &branch);
    store.save_workspace(&workspace)?;

    // Create pipeline state
    let pipeline_id = format!("build-{}", name);
    let pipeline = Pipeline::new_build(&pipeline_id, &name, &prompt);
    store.save_pipeline(&pipeline)?;

    // Generate CLAUDE.md
    let claude_md = generate_claude_md(&name, &prompt);
    std::fs::create_dir_all(&workspace_path)?;
    std::fs::write(workspace_path.join("CLAUDE.md"), claude_md)?;

    println!("Created build pipeline '{}'", name);
    println!("Workspace: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!();
    println!("Next steps:");
    println!("  1. Create the worktree: git worktree add {} -b {}", workspace_path.display(), branch);
    println!("  2. Start a Claude session in the workspace");
    println!("  3. Run `oj done` when the phase is complete");

    Ok(())
}

async fn run_bugfix(id: String) -> anyhow::Result<()> {
    let store = JsonStore::open(".build/operations")?;

    // Create workspace path
    let name = format!("bugfix-{}", id);
    let workspace_path = PathBuf::from(format!(".worktrees/{}", name));
    let branch = format!("bugfix-{}", id);

    // Create workspace state
    let workspace = Workspace::new(&name, &name, workspace_path.clone(), &branch);
    store.save_workspace(&workspace)?;

    // Create pipeline state
    let pipeline = Pipeline::new_bugfix(&name, &name, &id);
    store.save_pipeline(&pipeline)?;

    // Generate CLAUDE.md for bugfix
    let claude_md = generate_bugfix_claude_md(&id);
    std::fs::create_dir_all(&workspace_path)?;
    std::fs::write(workspace_path.join("CLAUDE.md"), claude_md)?;

    println!("Created bugfix pipeline for issue #{}", id);
    println!("Workspace: {}", workspace_path.display());
    println!("Branch: {}", branch);
    println!();
    println!("Next steps:");
    println!("  1. Create the worktree: git worktree add {} -b {}", workspace_path.display(), branch);
    println!("  2. Start a Claude session in the workspace");
    println!("  3. Run `oj done` when the phase is complete");

    Ok(())
}

fn generate_claude_md(name: &str, prompt: &str) -> String {
    format!(
        r#"# {name}

## Task
{prompt}

## Signaling

When you complete a phase, signal completion:
```bash
oj done
```

If you encounter an error you cannot resolve:
```bash
oj done --error "description of the issue"
```

## Environment

- `OTTER_TASK`: Current pipeline name
- `OTTER_WORKSPACE`: Workspace directory
- `OTTER_PHASE`: Current phase (plan, decompose, execute, fix, etc.)

## Guidelines

1. Work only within this workspace directory
2. Commit your changes before signaling completion
3. Signal `oj done` when the phase objective is complete
"#
    )
}

fn generate_bugfix_claude_md(issue_id: &str) -> String {
    format!(
        r#"# Bugfix for Issue #{issue_id}

## Task
Fix the issue described in issue #{issue_id}.

## Signaling

When you complete a phase, signal completion:
```bash
oj done
```

If you encounter an error you cannot resolve:
```bash
oj done --error "description of the issue"
```

## Guidelines

1. Understand the issue thoroughly before making changes
2. Write tests that reproduce the bug
3. Fix the bug
4. Verify all tests pass
5. Commit your changes before signaling completion
"#
    )
}
