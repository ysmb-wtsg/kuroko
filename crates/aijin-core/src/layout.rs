//! バイナリ分割ツリーによるペインレイアウトの管理。
//! Neovim/Zellijと同じ方式で、再帰的な水平・垂直分割を表現する。

use ratatui::layout::Rect;

use crate::types::{Direction, PaneId};

/// 分割面のセパレータ（方向と描画領域）
pub type Separator = (SplitDirection, Rect);

/// レイアウトの分割方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// 水平分割（上下に分ける）
    Horizontal,
    /// 垂直分割（左右に分ける）
    Vertical,
}

/// バイナリ分割ツリーのノード。
/// 各リーフがペインに対応し、内部ノードが分割を表現する。
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// ペインを表すリーフノード
    Leaf(PaneId),
    /// 分割を表す内部ノード
    Split {
        /// 分割方向
        direction: SplitDirection,
        /// 最初の子（左/上）が占める割合（0.0〜1.0）
        ratio: f32,
        /// 最初の子ノード（左/上）
        first: Box<LayoutNode>,
        /// 2番目の子ノード（右/下）
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// レイアウトツリーを解決し、各ペインの描画領域を算出する。
    ///
    /// @param area - 利用可能な描画領域全体
    /// @returns ペインIDと対応するRectのペアのリスト
    pub fn resolve(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        self.resolve_with_separators(area).0
    }

    /// レイアウトツリーを解決し、ペイン領域と分割面のセパレータ領域を算出する。
    /// 各分割面には1セル幅のセパレータが確保される（領域が小さすぎる場合は省略）。
    ///
    /// @param area - 利用可能な描画領域全体
    /// @returns (ペインIDとRectのリスト, セパレータの方向とRectのリスト)
    pub fn resolve_with_separators(
        &self,
        area: Rect,
    ) -> (Vec<(PaneId, Rect)>, Vec<Separator>) {
        let mut panes = Vec::new();
        let mut separators: Vec<Separator> = Vec::new();
        self.resolve_inner(area, &mut panes, &mut separators);
        (panes, separators)
    }

    /// resolveの再帰的な内部実装
    fn resolve_inner(
        &self,
        area: Rect,
        panes: &mut Vec<(PaneId, Rect)>,
        separators: &mut Vec<Separator>,
    ) {
        match self {
            LayoutNode::Leaf(id) => {
                panes.push((*id, area));
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_area, separator, second_area) = split_rect(area, *direction, *ratio);
                if let Some(sep) = separator {
                    separators.push((*direction, sep));
                }
                first.resolve_inner(first_area, panes, separators);
                second.resolve_inner(second_area, panes, separators);
            }
        }
    }

    /// 指定ペインを分割し、新しいペインを追加する。
    ///
    /// @param target - 分割対象のペインID
    /// @param new_pane - 新しく追加するペインID
    /// @param direction - 分割方向
    /// @returns 対象ペインが見つかればtrue
    pub fn split(
        &mut self,
        target: PaneId,
        new_pane: PaneId,
        direction: SplitDirection,
    ) -> bool {
        match self {
            LayoutNode::Leaf(id) if *id == target => {
                let old = LayoutNode::Leaf(*id);
                let new = LayoutNode::Leaf(new_pane);
                *self = LayoutNode::Split {
                    direction,
                    ratio: 0.5,
                    first: Box::new(old),
                    second: Box::new(new),
                };
                true
            }
            LayoutNode::Split { first, second, .. } => {
                first.split(target, new_pane, direction)
                    || second.split(target, new_pane, direction)
            }
            _ => false,
        }
    }

    /// 指定ペインを削除し、兄弟ノードで置き換える。
    ///
    /// @param target - 削除対象のペインID
    /// @returns 対象ペインが見つかればtrue
    pub fn remove(&mut self, target: PaneId) -> bool {
        match self {
            LayoutNode::Split { first, second, .. } => {
                if matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target) {
                    *self = *second.clone();
                    return true;
                }
                if matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target) {
                    *self = *first.clone();
                    return true;
                }
                first.remove(target) || second.remove(target)
            }
            _ => false,
        }
    }

    /// ツリー内の全ペインIDをDFS順で収集する。
    ///
    /// @returns ペインIDのリスト
    pub fn pane_ids(&self) -> Vec<PaneId> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    /// pane_idsの再帰的な内部実装
    fn collect_ids(&self, ids: &mut Vec<PaneId>) {
        match self {
            LayoutNode::Leaf(id) => ids.push(*id),
            LayoutNode::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }

    /// 指定方向にある隣接ペインを探す。
    /// レイアウトツリーのrect情報を使って空間的に隣接するペインを算出する。
    ///
    /// @param current - 現在フォーカス中のペインID
    /// @param direction - 移動方向
    /// @param total_area - 全体の描画領域
    /// @returns 隣接するペインのID（見つからなければNone）
    pub fn find_neighbor(
        &self,
        current: PaneId,
        direction: Direction,
        total_area: Rect,
    ) -> Option<PaneId> {
        let panes = self.resolve(total_area);
        let current_rect = panes.iter().find(|(id, _)| *id == current)?.1;

        // 現在のペインの中心座標
        let cx = current_rect.x as i32 + current_rect.width as i32 / 2;
        let cy = current_rect.y as i32 + current_rect.height as i32 / 2;

        panes
            .iter()
            .filter(|(id, _)| *id != current)
            .filter(|(_, rect)| {
                // 指定方向にあるペインのみをフィルタ
                let rx = rect.x as i32 + rect.width as i32 / 2;
                let ry = rect.y as i32 + rect.height as i32 / 2;
                match direction {
                    Direction::Left => rx < cx,
                    Direction::Right => rx > cx,
                    Direction::Up => ry < cy,
                    Direction::Down => ry > cy,
                }
            })
            .min_by_key(|(_, rect)| {
                // 最も近いペインを選ぶ（マンハッタン距離）
                let rx = rect.x as i32 + rect.width as i32 / 2;
                let ry = rect.y as i32 + rect.height as i32 / 2;
                (cx - rx).abs() + (cy - ry).abs()
            })
            .map(|(id, _)| *id)
    }

    /// 指定ペインを含むSplitノードのratioを変更する。
    ///
    /// @param target - リサイズ対象のペインID
    /// @param direction - リサイズ方向
    /// @param delta - ratio変化量（正で拡大、負で縮小）
    /// @returns 対象が見つかればtrue
    pub fn resize_pane(&mut self, target: PaneId, direction: Direction, delta: f32) -> bool {
        match self {
            LayoutNode::Split {
                direction: split_dir,
                ratio,
                first,
                second,
            } => {
                let first_contains = first.contains(target);
                let second_contains = second.contains(target);

                if first_contains || second_contains {
                    // このSplitが対象ペインの直接の親かチェック
                    let is_direct_parent = matches!(first.as_ref(), LayoutNode::Leaf(id) if *id == target)
                        || matches!(second.as_ref(), LayoutNode::Leaf(id) if *id == target);

                    if is_direct_parent {
                        // 分割方向とリサイズ方向が一致する場合のみratioを変更
                        let should_resize = matches!(
                            (split_dir, direction),
                            (SplitDirection::Vertical, Direction::Left | Direction::Right)
                                | (SplitDirection::Horizontal, Direction::Up | Direction::Down)
                        );

                        if should_resize {
                            let adjust = if first_contains { delta } else { -delta };
                            *ratio = (*ratio + adjust).clamp(0.1, 0.9);
                            return true;
                        }
                    }

                    // 再帰的に子ノードを探索
                    first.resize_pane(target, direction, delta)
                        || second.resize_pane(target, direction, delta)
                } else {
                    false
                }
            }
            LayoutNode::Leaf(_) => false,
        }
    }

    /// このノードが指定ペインIDを含むかどうかを返す
    fn contains(&self, target: PaneId) -> bool {
        match self {
            LayoutNode::Leaf(id) => *id == target,
            LayoutNode::Split { first, second, .. } => {
                first.contains(target) || second.contains(target)
            }
        }
    }
}

/// Rectを指定方向・比率で2つに分割し、間に1セル幅のセパレータ領域を確保する。
/// 分割軸の長さが3セル未満の場合はセパレータを省略して従来通り2分割する。
///
/// @param area - 分割対象の領域
/// @param direction - 分割方向
/// @param ratio - 最初の領域が占める割合
/// @returns (最初の領域, セパレータ領域, 2番目の領域)
fn split_rect(area: Rect, direction: SplitDirection, ratio: f32) -> (Rect, Option<Rect>, Rect) {
    match direction {
        SplitDirection::Horizontal => {
            if area.height < 3 {
                let first_height = (area.height as f32 * ratio) as u16;
                let second_height = area.height.saturating_sub(first_height);
                return (
                    Rect::new(area.x, area.y, area.width, first_height),
                    None,
                    Rect::new(area.x, area.y + first_height, area.width, second_height),
                );
            }
            let usable = area.height - 1;
            let first_height = (usable as f32 * ratio) as u16;
            let second_height = usable - first_height;
            (
                Rect::new(area.x, area.y, area.width, first_height),
                Some(Rect::new(area.x, area.y + first_height, area.width, 1)),
                Rect::new(area.x, area.y + first_height + 1, area.width, second_height),
            )
        }
        SplitDirection::Vertical => {
            if area.width < 3 {
                let first_width = (area.width as f32 * ratio) as u16;
                let second_width = area.width.saturating_sub(first_width);
                return (
                    Rect::new(area.x, area.y, first_width, area.height),
                    None,
                    Rect::new(area.x + first_width, area.y, second_width, area.height),
                );
            }
            let usable = area.width - 1;
            let first_width = (usable as f32 * ratio) as u16;
            let second_width = usable - first_width;
            (
                Rect::new(area.x, area.y, first_width, area.height),
                Some(Rect::new(area.x + first_width, area.y, 1, area.height)),
                Rect::new(area.x + first_width + 1, area.y, second_width, area.height),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の100x50領域
    fn area() -> Rect {
        Rect::new(0, 0, 100, 50)
    }

    // --- resolve ---

    #[test]
    fn resolve_single_leaf() {
        let node = LayoutNode::Leaf(PaneId(1));
        let result = node.resolve(area());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], (PaneId(1), area()));
    }

    #[test]
    fn resolve_vertical_split() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        let (result, separators) = node.resolve_with_separators(area());
        assert_eq!(result.len(), 2);
        // 左半分: 幅49（セパレータ1セルを除いた99の半分）
        assert_eq!(result[0].0, PaneId(1));
        assert_eq!(result[0].1, Rect::new(0, 0, 49, 50));
        // 右半分: 幅50
        assert_eq!(result[1].0, PaneId(2));
        assert_eq!(result[1].1, Rect::new(50, 0, 50, 50));
        // セパレータ: x=49に1セル幅
        assert_eq!(separators.len(), 1);
        assert_eq!(separators[0], (SplitDirection::Vertical, Rect::new(49, 0, 1, 50)));
    }

    #[test]
    fn resolve_horizontal_split() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        let (result, separators) = node.resolve_with_separators(area());
        assert_eq!(result.len(), 2);
        // 上半分: 高さ24（セパレータ1セルを除いた49の半分）
        assert_eq!(result[0].1, Rect::new(0, 0, 100, 24));
        // 下半分: 高さ25
        assert_eq!(result[1].1, Rect::new(0, 25, 100, 25));
        // セパレータ: y=24に1行
        assert_eq!(separators.len(), 1);
        assert_eq!(separators[0], (SplitDirection::Horizontal, Rect::new(0, 24, 100, 1)));
    }

    #[test]
    fn resolve_nested_split() {
        // 垂直分割の左側をさらに水平分割
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(PaneId(1))),
                second: Box::new(LayoutNode::Leaf(PaneId(2))),
            }),
            second: Box::new(LayoutNode::Leaf(PaneId(3))),
        };
        let (result, separators) = node.resolve_with_separators(area());
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].1, Rect::new(0, 0, 49, 24));
        assert_eq!(result[1].1, Rect::new(0, 25, 49, 25));
        assert_eq!(result[2].1, Rect::new(50, 0, 50, 50));
        // 垂直分割面 + 左側の水平分割面の2本
        assert_eq!(separators.len(), 2);
        assert_eq!(separators[0], (SplitDirection::Vertical, Rect::new(49, 0, 1, 50)));
        assert_eq!(separators[1], (SplitDirection::Horizontal, Rect::new(0, 24, 49, 1)));
    }

    #[test]
    fn split_rect_too_small_omits_separator() {
        // 分割軸の長さが3セル未満ならセパレータなしで2分割する
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        let (result, separators) = node.resolve_with_separators(Rect::new(0, 0, 2, 10));
        assert_eq!(result.len(), 2);
        assert!(separators.is_empty());
        assert_eq!(result[0].1, Rect::new(0, 0, 1, 10));
        assert_eq!(result[1].1, Rect::new(1, 0, 1, 10));
    }

    // --- split ---

    #[test]
    fn split_leaf_creates_split_node() {
        let mut node = LayoutNode::Leaf(PaneId(1));
        assert!(node.split(PaneId(1), PaneId(2), SplitDirection::Vertical));
        let ids = node.pane_ids();
        assert_eq!(ids, vec![PaneId(1), PaneId(2)]);
    }

    #[test]
    fn split_nonexistent_target_returns_false() {
        let mut node = LayoutNode::Leaf(PaneId(1));
        assert!(!node.split(PaneId(99), PaneId(2), SplitDirection::Vertical));
    }

    #[test]
    fn split_nested_target() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(node.split(PaneId(2), PaneId(3), SplitDirection::Horizontal));
        assert_eq!(node.pane_ids(), vec![PaneId(1), PaneId(2), PaneId(3)]);
    }

    // --- remove ---

    #[test]
    fn remove_from_split_promotes_sibling() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(node.remove(PaneId(1)));
        assert_eq!(node.pane_ids(), vec![PaneId(2)]);
        // 兄弟が昇格してLeafになる
        assert!(matches!(node, LayoutNode::Leaf(PaneId(2))));
    }

    #[test]
    fn remove_second_child_promotes_first() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(node.remove(PaneId(2)));
        assert!(matches!(node, LayoutNode::Leaf(PaneId(1))));
    }

    #[test]
    fn remove_from_leaf_returns_false() {
        let mut node = LayoutNode::Leaf(PaneId(1));
        assert!(!node.remove(PaneId(1)));
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(!node.remove(PaneId(99)));
    }

    // --- pane_ids ---

    #[test]
    fn pane_ids_dfs_order() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                ratio: 0.5,
                first: Box::new(LayoutNode::Leaf(PaneId(1))),
                second: Box::new(LayoutNode::Leaf(PaneId(2))),
            }),
            second: Box::new(LayoutNode::Leaf(PaneId(3))),
        };
        assert_eq!(node.pane_ids(), vec![PaneId(1), PaneId(2), PaneId(3)]);
    }

    // --- find_neighbor ---

    #[test]
    fn find_neighbor_right() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert_eq!(
            node.find_neighbor(PaneId(1), Direction::Right, area()),
            Some(PaneId(2))
        );
    }

    #[test]
    fn find_neighbor_left() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert_eq!(
            node.find_neighbor(PaneId(2), Direction::Left, area()),
            Some(PaneId(1))
        );
    }

    #[test]
    fn find_neighbor_no_match_returns_none() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        // 左右分割なので上下の隣接はない
        assert_eq!(
            node.find_neighbor(PaneId(1), Direction::Up, area()),
            None
        );
    }

    #[test]
    fn find_neighbor_down_in_horizontal_split() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert_eq!(
            node.find_neighbor(PaneId(1), Direction::Down, area()),
            Some(PaneId(2))
        );
    }

    #[test]
    fn find_neighbor_nonexistent_current_returns_none() {
        let node = LayoutNode::Leaf(PaneId(1));
        assert_eq!(
            node.find_neighbor(PaneId(99), Direction::Right, area()),
            None
        );
    }

    // --- resize_pane ---

    #[test]
    fn resize_pane_adjusts_ratio() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(node.resize_pane(PaneId(1), Direction::Right, 0.1));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((ratio - 0.6).abs() < f32::EPSILON);
        } else {
            panic!("expected Split node");
        }
    }

    #[test]
    fn resize_pane_clamps_ratio() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.85,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        // 0.85 + 0.2 = 1.05 -> clamp to 0.9
        assert!(node.resize_pane(PaneId(1), Direction::Right, 0.2));
        if let LayoutNode::Split { ratio, .. } = &node {
            assert!((ratio - 0.9).abs() < f32::EPSILON);
        } else {
            panic!("expected Split node");
        }
    }

    #[test]
    fn resize_pane_wrong_direction_returns_false() {
        let mut node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        // 垂直分割に対して上下リサイズは効果なし
        assert!(!node.resize_pane(PaneId(1), Direction::Up, 0.1));
    }

    #[test]
    fn resize_pane_leaf_returns_false() {
        let mut node = LayoutNode::Leaf(PaneId(1));
        assert!(!node.resize_pane(PaneId(1), Direction::Right, 0.1));
    }

    // --- contains ---

    #[test]
    fn contains_finds_existing_pane() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        assert!(node.contains(PaneId(1)));
        assert!(node.contains(PaneId(2)));
        assert!(!node.contains(PaneId(99)));
    }

    // --- split_rect (indirect via resolve) ---

    #[test]
    fn resolve_unequal_ratio() {
        let node = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            ratio: 0.3,
            first: Box::new(LayoutNode::Leaf(PaneId(1))),
            second: Box::new(LayoutNode::Leaf(PaneId(2))),
        };
        let result = node.resolve(area());
        // セパレータ1セルを除いた99 * 0.3 = 29
        assert_eq!(result[0].1.width, 29);
        assert_eq!(result[1].1.width, 70);
        assert_eq!(result[1].1.x, 30);
    }
}
