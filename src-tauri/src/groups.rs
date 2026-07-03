//! Persistencia de la lista de grupos de proyectos.
//!
//! Un proyecto guarda su grupo en `config.group` (un `Option<String>`), pero eso
//! solo permite grupos «derivados»: un grupo existe mientras algún proyecto lo
//! tenga. Para crear grupos vacíos, fijar su orden y renombrarlos/borrarlos sin
//! depender de los proyectos, guardamos la lista en `config_dir()/groups.json`.
//!
//! La fuente de verdad de la *pertenencia* sigue siendo `config.group`; este
//! archivo solo aporta el conjunto de grupos conocidos y su orden de aparición.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Default, Serialize, Deserialize)]
struct GroupsFile {
    /// Nombres de grupo en el orden en que se muestran.
    #[serde(default)]
    order: Vec<String>,
}

fn groups_path() -> Result<PathBuf> {
    Ok(config::config_dir()?.join("groups.json"))
}

fn read_file() -> Result<GroupsFile> {
    let path = groups_path()?;
    if !path.exists() {
        return Ok(GroupsFile::default());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("leyendo {:?}", path))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn write_file(f: &GroupsFile) -> Result<()> {
    let path = groups_path()?;
    let raw = serde_json::to_string_pretty(f)?;
    std::fs::write(&path, raw).with_context(|| format!("escribiendo {:?}", path))?;
    Ok(())
}

/// Lista de grupos persistidos, en orden. No incluye grupos que solo existan en
/// `config.group` de algún proyecto (de la fusión se encarga el frontend).
pub fn list() -> Result<Vec<String>> {
    Ok(read_file()?.order)
}

/// Añade un grupo al final si no existe ya (comparación exacta tras `trim`).
/// Idempotente: crear un grupo que ya está no cambia nada.
pub fn create(name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(());
    }
    let mut f = read_file()?;
    if !f.order.iter().any(|g| g == name) {
        f.order.push(name.to_string());
        write_file(&f)?;
    }
    Ok(())
}

/// Renombra un grupo en la lista y reasigna `config.group` en cada proyecto que
/// lo tenga. No-op si `old` no existe o `new` está vacío.
pub fn rename(old: &str, new: &str) -> Result<()> {
    let new = new.trim();
    if new.is_empty() || old == new {
        return Ok(());
    }
    let mut f = read_file()?;
    let mut changed = false;
    for g in f.order.iter_mut() {
        if g == old {
            *g = new.to_string();
            changed = true;
        }
    }
    // Quita un posible duplicado si `new` ya existía.
    if changed {
        let mut seen = std::collections::HashSet::new();
        f.order.retain(|g| seen.insert(g.clone()));
        write_file(&f)?;
    }
    // Reasigna los proyectos que apuntaban al grupo viejo.
    for mut site in config::load_all_sites()? {
        if site.group.as_deref() == Some(old) {
            site.group = Some(new.to_string());
            config::write_site_config(&site)?;
        }
    }
    Ok(())
}

/// Quita un grupo de la lista; los proyectos que lo tenían quedan sin grupo.
pub fn delete(name: &str) -> Result<()> {
    let mut f = read_file()?;
    let before = f.order.len();
    f.order.retain(|g| g != name);
    if f.order.len() != before {
        write_file(&f)?;
    }
    for mut site in config::load_all_sites()? {
        if site.group.as_deref() == Some(name) {
            site.group = None;
            config::write_site_config(&site)?;
        }
    }
    Ok(())
}

/// Sobrescribe el orden de los grupos (drag de cabeceras de grupo).
pub fn reorder(order: Vec<String>) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let order: Vec<String> = order
        .into_iter()
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty() && seen.insert(g.clone()))
        .collect();
    write_file(&GroupsFile { order })
}

#[cfg(test)]
mod tests {
    //! Estos tests mutan `config_dir()` real, así que se marcan `#[ignore]`
    //! (se ejecutan con `cargo test -- --ignored --test-threads=1`).
    use super::*;

    #[test]
    #[ignore]
    fn create_is_idempotent() {
        let _ = std::fs::remove_file(groups_path().unwrap());
        create("alpha").unwrap();
        create("alpha").unwrap();
        create("beta").unwrap();
        assert_eq!(list().unwrap(), vec!["alpha", "beta"]);
        let _ = std::fs::remove_file(groups_path().unwrap());
    }

    #[test]
    #[ignore]
    fn delete_removes_from_list() {
        let _ = std::fs::remove_file(groups_path().unwrap());
        create("x").unwrap();
        create("y").unwrap();
        delete("x").unwrap();
        assert_eq!(list().unwrap(), vec!["y"]);
        let _ = std::fs::remove_file(groups_path().unwrap());
    }

    #[test]
    #[ignore]
    fn reorder_dedups_and_trims() {
        reorder(vec![" a ".into(), "b".into(), "a".into(), "".into()]).unwrap();
        assert_eq!(list().unwrap(), vec!["a", "b"]);
        let _ = std::fs::remove_file(groups_path().unwrap());
    }
}
