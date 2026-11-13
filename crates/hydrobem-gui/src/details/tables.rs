//! The tables of the input nodes, and their edits.

use std::collections::HashMap;

use gui_core::hydrobem_engine::{QualityFlags, wavenumber};
use gui_core::project::{DOF_LABELS, Project, WaveInputType};

use super::Derived;
use super::body::{control_points, matrix_table, mode_terms};
use crate::output::{Cell, CellEdit, Column, Output, TableData, Tabs};
use crate::tree::{BodyPart, Node};

// ---- The tables of the input nodes -------------------------------------------------------------

pub fn output(project: &Project, node: Node, derived: &Derived) -> Output {
    match node {
        Node::FieldPoints if project.field_points.list.is_some() => {
            let tanks = project.all_tanks();
            let list = project.field_points.list.as_deref().unwrap_or(&[]);
            let mut columns = vec![Column::new("Number")];
            columns.extend(["X [m]", "Y [m]", "Z [m]", "Tank"].map(|c| Column::editable(c, true)));
            columns.push(Column::new("Tank Name"));
            let rows = list
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let k = p.tank.as_ref().and_then(|t| tanks.iter().position(|(x, _)| x == t)).map_or(0, |k| k + 1);
                    let name = p.tank.as_ref().map(|t| project.tank_path(t)).unwrap_or_else(|| "exterior".into());
                    vec![
                        Cell::Int(i as i64 + 1),
                        Cell::Num(p.point[0]),
                        Cell::Num(p.point[1]),
                        Cell::Num(p.point[2]),
                        Cell::Int(k as i64),
                        Cell::Text(name),
                    ]
                })
                .collect();
            Output {
                title: "Field Points [global]".into(),
                table: Some(TableData { columns, rows }),
                plot: None,
                tabs: None,
            }
        }
        Node::BodyPart(i, BodyPart::Generalized(k)) => {
            let g = project.bodies.get(i).and_then(|b| b.generalized_modes.get(k));
            let rows: Vec<Vec<Cell>> = mode_terms(g)
                .into_iter()
                .map(|(j, c, t)| {
                    let mut row = vec![Cell::Int(j as i64 + 1), Cell::Text(["u", "v", "w"][c].into())];
                    row.extend(t.iter().map(|x| Cell::Num(*x)));
                    row
                })
                .collect();
            Output {
                title: "Generalized Mode Terms c x^px y^py z^pz [body-local]".into(),
                table: (!rows.is_empty()).then(|| TableData {
                    columns: vec![
                        Column::new("Piece"),
                        Column::new("Component"),
                        Column::editable("c", true),
                        Column::editable("px", true),
                        Column::editable("py", true),
                        Column::editable("pz", true),
                    ],
                    rows,
                }),
                plot: None,
                tabs: None,
            }
        }
        Node::BodyPart(i, BodyPart::ModeMatrices) => {
            let b = project.bodies.get(i);
            let n = 6 + b.map_or(0, |b| b.generalized_modes.len());
            let m = b.and_then(|b| b.mode_matrices.as_ref());
            let values = m.map(|m| match m.shown {
                1 => &m.damping,
                2 => &m.stiffness,
                _ => &m.mass,
            });
            let label = |k: usize| if k < 6 { DOF_LABELS[k].to_string() } else { format!("G{}", k - 5) };
            let mut columns = vec![Column::new("Row")];
            columns.extend((0..n).map(|k| Column::editable(label(k), true)));
            Output {
                title: format!(
                    "Mode {} [{n} x {n}]",
                    ["Mass", "Damping", "Stiffness"][m.map_or(0, |m| m.shown.min(2))]
                ),
                table: values.filter(|v| v.len() == n * n).map(|v| TableData {
                    columns,
                    rows: (0..n)
                        .map(|r| {
                            std::iter::once(Cell::Text(label(r)))
                                .chain((0..n).map(|c| Cell::Num(v[r * n + c])))
                                .collect()
                        })
                        .collect(),
                }),
                plot: None,
                tabs: None,
            }
        }
        Node::BodyPart(i, BodyPart::ControlSurface) => {
            let c = project.bodies.get(i).and_then(|b| b.control_surface.as_ref());
            let rows: Vec<Vec<Cell>> = control_points(c)
                .into_iter()
                .map(|(line, j, p)| {
                    vec![Cell::Int(line as i64), Cell::Int(j as i64 + 1), Cell::Num(p[0]), Cell::Num(p[1])]
                })
                .collect();
            Output {
                title: "Control Surface Boundary (Line 0) and Partitions [body-local]".into(),
                table: (!rows.is_empty()).then(|| TableData {
                    columns: vec![
                        Column::new("Line"),
                        Column::new("Point"),
                        Column::editable("X [m]", true),
                        Column::editable("Y [m]", true),
                    ],
                    rows,
                }),
                plot: None,
                tabs: None,
            }
        }
        Node::BodyPart(i, BodyPart::BodyPoints) => {
            let points = project.bodies.get(i).and_then(|b| b.body_points.as_ref());
            Output {
                title: "Body Points [body-local]".into(),
                table: Some(TableData {
                    columns: vec![
                        Column::new("Number"),
                        Column::editable("X [m]", true),
                        Column::editable("Y [m]", true),
                        Column::editable("Z [m]", true),
                    ],
                    rows: points
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(|(r, p)| vec![Cell::Int(r as i64 + 1), Cell::Num(p[0]), Cell::Num(p[1]), Cell::Num(p[2])])
                        .collect(),
                }),
                plot: None,
                tabs: None,
            }
        }
        Node::Meshes => meshes_table(project, derived),
        Node::Mesh(i) => mesh_tables(project, i, derived),
        Node::BodyPart(i, part @ (BodyPart::Damping | BodyPart::Stiffness)) => {
            let damping = part == BodyPart::Damping;
            let m =
                if damping { &project.bodies[i].additional_damping } else { &project.bodies[i].additional_stiffness };
            let Some(m) = m else { return Output::default() };
            matrix_table(format!("{} [6 × 6]", part.label()), m)
        }
        Node::BodyPart(i, BodyPart::Coupling(k)) => {
            let Some(c) = project.bodies.get(i).and_then(|b| b.couplings.get(k)) else { return Output::default() };
            let (m, what) = if c.shown == 1 { (&c.damping, "Damping") } else { (&c.stiffness, "Stiffness") };
            matrix_table(format!("Coupling {} {what}, {} → {} [6 × 6]", k + 1, c.body, project.bodies[i].name), m)
        }
        Node::Headings => Output {
            title: "Headings".into(),
            table: Some(TableData {
                columns: vec![Column::new("Number"), Column::new("Wave Heading [deg]")],
                rows: project
                    .waves
                    .headings
                    .values()
                    .iter()
                    .enumerate()
                    .map(|(i, h)| vec![Cell::Int(i as i64 + 1), Cell::Num(*h)])
                    .collect(),
            }),
            plot: None,
            tabs: None,
        },
        Node::Frequencies => {
            let f = &project.waves.frequencies;
            let period = f.input_type == WaveInputType::Period;
            let (fh, ph) = ("Wave Frequency [rad/s]", "Period [s]");
            let columns = vec![
                Column::new("Number"),
                Column::editable(if period { ph } else { fh }, f.edit_in_table),
                Column::new(if period { fh } else { ph }),
                Column::new("Wave Number [1/m]"),
                Column::new("Wave Length [m]"),
            ];
            let (depth, g) = (project.environment.engine_depth(), project.environment.gravity);
            let rows = (0..f.inputs.len())
                .map(|r| {
                    let omega = f.omega(r);
                    let k = wavenumber(omega, depth, g).unwrap_or(f64::NAN);
                    vec![
                        Cell::Int(r as i64 + 1),
                        Cell::Num(f.inputs[r]),
                        Cell::Num(if period { omega } else { f.period(r) }),
                        Cell::Num(k),
                        Cell::Num(if k > 0.0 { std::f64::consts::TAU / k } else { f64::NAN }),
                    ]
                })
                .collect();
            Output { title: "Frequencies".into(), table: Some(TableData { columns, rows }), plot: None, tabs: None }
        }
        _ => Output::default(),
    }
}

/// The mesh library, one row per mesh: what it is and what the engine says about it alone.
fn meshes_table(project: &Project, derived: &Derived) -> Output {
    let columns = [
        "Number",
        "Name",
        "Kind",
        "Symmetry",
        "Records",
        "Volume [m³]",
        "Area [m²]",
        "Draft [m]",
        "Inconsistent Orientation",
        "Free Edges",
        "Non-Manifold Edges",
        "Used By",
    ]
    .map(Column::new)
    .to_vec();
    let rows = project
        .meshes
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let mesh = &entry.mesh;
            let info = derived.meshes.get(&mesh.id).and_then(|r| r.as_ref().ok());
            let count = |f: QualityFlags| match info {
                Some(m) => Cell::Int(m.quality.flags.iter().filter(|x| x.contains(f)).count() as i64),
                None => Cell::Text(String::new()),
            };
            let number = |v: Option<f64>| v.map_or(Cell::Text(String::new()), Cell::Num);
            let users = project.mesh_users(i);
            vec![
                Cell::Int(i as i64 + 1),
                Cell::Text(entry.name.clone()),
                Cell::Text(mesh_kind(mesh).into()),
                Cell::Text(mesh.symmetry.label().into()),
                Cell::Int(mesh.panels.len() as i64),
                number(info.map(|m| m.geometry.volume)),
                number(info.map(|m| m.geometry.area)),
                number(info.map(|m| (-m.geometry.bounds[0][2]).max(0.0))),
                count(QualityFlags::ORIENTATION),
                count(QualityFlags::FREE_EDGE),
                count(QualityFlags::NON_MANIFOLD),
                Cell::Text(if users.is_empty() { "(unused)".into() } else { users.join(", ") }),
            ]
        })
        .collect();
    Output { title: "Meshes".into(), table: Some(TableData { columns, rows }), plot: None, tabs: None }
}

pub(super) fn mesh_kind(mesh: &gui_core::mesh::Mesh) -> &'static str {
    match mesh.patch.as_ref().map(|p| p.kind.as_str()) {
        None => "Low order: panels",
        Some("flat_patches") => "Higher order: flat patches",
        Some("spline_patches") => "Higher order: B-spline patches",
        Some(_) => "Higher order: analytic patches",
    }
}

/// The quality flags of a record as text.
fn flag_text(flags: QualityFlags) -> String {
    [
        (QualityFlags::ORIENTATION, "orientation"),
        (QualityFlags::FREE_EDGE, "free edge"),
        (QualityFlags::NON_MANIFOLD, "non-manifold"),
        (QualityFlags::COARSE, "coarse"),
    ]
    .iter()
    .filter(|(f, _)| flags.contains(*f))
    .map(|(_, t)| *t)
    .collect::<Vec<_>>()
    .join(", ")
}

/// The tables of a library mesh, as tabs: low order its panels and their vertices as stored; a
/// patch mesh its discretization (editable), its patches and their corners. The centroids,
/// normals (into the fluid) and areas are the engine's.
fn mesh_tables(project: &Project, i: usize, derived: &Derived) -> Output {
    let Some(entry) = project.meshes.get(i) else { return Output::default() };
    let mesh = &entry.mesh;
    let info = derived.meshes.get(&mesh.id).and_then(|r| r.as_ref().ok());
    let patch = mesh.is_patch();
    // the engine's rows by stored record
    let by_record: HashMap<usize, usize> = info
        .map(|m| m.elements.element.iter().enumerate().map(|(k, r)| ((*r).max(1) as usize - 1, k)).collect())
        .unwrap_or_default();
    let item = if patch { "Patch" } else { "Number" };
    let mut columns = vec![Column::new(item)];
    columns.extend(
        ["Xc [m]", "Yc [m]", "Zc [m]", "nx", "ny", "nz", "Area [m²]", "Size [m]", "Aspect", "Flags"].map(Column::new),
    );
    let rows = (0..mesh.panels.len())
        .map(|r| {
            let k = by_record.get(&r).copied();
            let rec = info.map(|m| &m.elements);
            let triple = |v: Option<&Vec<[f64; 3]>>| k.and_then(|k| v?.get(k).copied()).unwrap_or([f64::NAN; 3]);
            let c = triple(rec.map(|x| &x.centroids));
            let n = triple(rec.map(|x| &x.normals));
            let area = k.and_then(|k| rec?.areas.get(k).copied()).unwrap_or(f64::NAN);
            let per = |v: Option<&Vec<f64>>| v.and_then(|v| v.get(r)).copied().unwrap_or(f64::NAN);
            let q = info.map(|m| &m.quality);
            let flags = q.and_then(|q| q.flags.get(r)).copied().map(flag_text).unwrap_or_default();
            vec![
                Cell::Int(r as i64 + 1),
                Cell::Num(c[0]),
                Cell::Num(c[1]),
                Cell::Num(c[2]),
                Cell::Fixed(n[0], 4),
                Cell::Fixed(n[1], 4),
                Cell::Fixed(n[2], 4),
                Cell::Num(area),
                Cell::Num(per(q.map(|q| &q.size))),
                Cell::Num(per(q.map(|q| &q.aspect))),
                Cell::Text(flags),
            ]
        })
        .collect();
    let records = TableData { columns, rows };
    let corner = if patch { "Corner" } else { "Vertex" };
    let vertices = TableData {
        columns: [item, corner, "X [m]", "Y [m]", "Z [m]"].map(Column::new).to_vec(),
        rows: mesh
            .panels
            .iter()
            .enumerate()
            .flat_map(|(r, q)| {
                q.iter().enumerate().map(move |(v, p)| {
                    vec![
                        Cell::Int(r as i64 + 1),
                        Cell::Int(v as i64 + 1),
                        Cell::Num(p[0]),
                        Cell::Num(p[1]),
                        Cell::Num(p[2]),
                    ]
                })
            })
            .collect(),
    };
    if !patch {
        return Output {
            title: format!("{} · stored panels", entry.name),
            table: Some(records),
            plot: None,
            tabs: Some(Tabs {
                names: vec!["Panels".into(), "Vertices".into()],
                more: vec![vertices],
                rows_per_record: vec![1, 4],
            }),
        };
    }
    let mut columns = vec![Column::new("Patch")];
    columns.extend(["nu", "nv", "ku", "kv", "Outer Gauss", "Inner Gauss"].map(|c| Column::editable(c, true)));
    // the patch names (of a geometry file, cad2bem's CAD faces) after the editable columns
    let names = mesh.patch_names();
    let named = names.iter().any(|n| !n.is_empty());
    if named {
        columns.push(Column::new("Name"));
    }
    // cad2bem's report: the CAD face and the checks of each patch
    let report = mesh.report.as_deref();
    if report.is_some() {
        columns.extend(["CAD Face", "Face Type", "State", "Checks"].map(Column::new));
    }
    let rows = mesh
        .discretization()
        .iter()
        .enumerate()
        .map(|(r, d)| {
            let mut row: Vec<Cell> =
                std::iter::once(Cell::Int(r as i64 + 1)).chain(d.iter().map(|v| Cell::Int(*v))).collect();
            if named {
                row.push(Cell::Text(names.get(r).cloned().unwrap_or_default()));
            }
            if let Some(p) = report.and_then(|rep| rep.patches.get(r)) {
                row.push(Cell::Int(p.face));
                row.push(Cell::Text(p.face_type.clone()));
                row.push(Cell::Text(p.state.clone()));
                row.push(Cell::Text(p.problems.join(", ")));
            }
            row
        })
        .collect();
    Output {
        title: format!("{} · patches", entry.name),
        table: Some(TableData { columns, rows }),
        plot: None,
        tabs: Some(Tabs {
            names: vec!["Discretization".into(), "Patches".into(), "Corners".into()],
            more: vec![records, vertices],
            rows_per_record: vec![1, 1, 4],
        }),
    }
}

/// A table edit of an input node; false when refused.
pub fn apply_edit(project: &mut Project, node: Node, edit: CellEdit) -> bool {
    match node {
        Node::Mesh(i) if (1..=6).contains(&edit.column) => {
            let Some(entry) = project.meshes.get(i) else { return false };
            let value = edit.value.round() as i64;
            if value < 1 {
                return false;
            }
            let Some(mesh) = entry.mesh.with_discretization(edit.row, edit.column - 1, value) else { return false };
            project.replace_mesh(i, std::sync::Arc::new(mesh));
            project.update_mesh_properties();
            true
        }
        Node::FieldPoints if (1..=4).contains(&edit.column) => {
            let tanks = project.all_tanks();
            let Some(p) = project.field_points.list.as_mut().and_then(|l| l.get_mut(edit.row)) else { return false };
            if edit.column == 4 {
                // 0 the exterior, k the k-th tank of the bodies
                let k = edit.value.round();
                if k < 0.0 || k as usize > tanks.len() {
                    return false;
                }
                p.tank = (k as usize).checked_sub(1).map(|k| tanks[k].0.clone());
            } else {
                p.point[edit.column - 1] = edit.value;
            }
            true
        }
        Node::BodyPart(i, BodyPart::Generalized(k)) if (2..=5).contains(&edit.column) => {
            let Some(g) = project.bodies[i].generalized_modes.get_mut(k) else { return false };
            let Some((j, c, _)) = mode_terms(Some(g)).into_iter().nth(edit.row) else { return false };
            let before: usize = g.pieces[j].terms[..c].iter().map(Vec::len).sum();
            let offset = edit.row
                - g.pieces[..j].iter().map(|p| p.terms.iter().map(Vec::len).sum::<usize>()).sum::<usize>()
                - before;
            g.pieces[j].terms[c][offset][edit.column - 2] = edit.value;
            true
        }
        Node::BodyPart(i, BodyPart::ModeMatrices) if edit.column >= 1 => {
            let n = 6 + project.bodies[i].generalized_modes.len();
            let Some(m) = project.bodies[i].mode_matrices.as_mut() else { return false };
            let v = match m.shown {
                1 => &mut m.damping,
                2 => &mut m.stiffness,
                _ => &mut m.mass,
            };
            match v.get_mut(edit.row * n + edit.column - 1) {
                Some(x) if edit.row < n && edit.column - 1 < n => {
                    *x = edit.value;
                    true
                }
                _ => false,
            }
        }
        Node::BodyPart(i, BodyPart::ControlSurface) if (2..=3).contains(&edit.column) => {
            let Some(c) = project.bodies[i].control_surface.as_mut() else { return false };
            let Some((line, j, _)) = control_points(Some(c)).into_iter().nth(edit.row) else { return false };
            let point =
                if line == 0 { c.boundary.as_mut().map(|b| &mut b[j]) } else { Some(&mut c.partitions[line - 1][j]) };
            match point {
                Some(p) => {
                    p[edit.column - 2] = edit.value;
                    true
                }
                None => false,
            }
        }
        Node::BodyPart(i, BodyPart::BodyPoints) if (1..=3).contains(&edit.column) => {
            match project.bodies[i].body_points.as_mut().and_then(|p| p.get_mut(edit.row)) {
                Some(p) => {
                    p[edit.column - 1] = edit.value;
                    true
                }
                None => false,
            }
        }
        Node::BodyPart(i, BodyPart::Coupling(k)) if edit.column >= 1 => {
            let Some(c) = project.bodies[i].couplings.get_mut(k) else { return false };
            let m = if c.shown == 1 { &mut c.damping } else { &mut c.stiffness };
            if !m.edit_in_table || edit.row >= 6 || edit.column > 6 {
                return false;
            }
            m.set(edit.row, edit.column - 1, edit.value);
            true
        }
        Node::BodyPart(i, part @ (BodyPart::Damping | BodyPart::Stiffness)) if edit.column >= 1 => {
            let b = &mut project.bodies[i];
            let m = if part == BodyPart::Damping { &mut b.additional_damping } else { &mut b.additional_stiffness };
            let Some(m) = m else { return false };
            if !m.edit_in_table {
                return false;
            }
            m.set(edit.row, edit.column - 1, edit.value);
            true
        }
        Node::Frequencies if edit.column == 1 && edit.value > 0.0 && project.waves.frequencies.edit_in_table => {
            project.waves.frequencies.inputs[edit.row] = edit.value;
            true
        }
        _ => false,
    }
}
