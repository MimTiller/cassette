//! Who may give or take a role: the server rules as pure functions.
//!
//! The rules are the same ones the client's role picker uses to decide which
//! roles to offer, so the picker and the enforcement cannot drift.
//!
//! A member may hand out or remove only a role positioned strictly BELOW their
//! own highest role, and only with Manage Roles; the owner may do everything,
//! and nobody but the owner may change the owner's roles.

/// How high a member ranks: their role's position, or `None` when they hold no
/// roles (a member with no roles may manage none).
pub type RolePosition = i64;

/// What the caller may do, without the DB.
pub struct Actor {
    pub is_owner: bool,
    pub manage_roles: bool,
    pub highest_position: Option<RolePosition>,
}

/// May `actor` hand out the role at `position`? The owner may; otherwise the
/// caller needs Manage Roles and the role must sit strictly below their highest.
pub fn can_give(actor: &Actor, position: RolePosition) -> Result<(), String> {
    if actor.is_owner {
        return Ok(())
    }
    if !actor.manage_roles {
        return Err("you need Manage Roles to assign roles".into())
    }
    let below = match actor.highest_position {
        None => false,
        Some(h) => position < h,
    };
    if !below {
        return Err("you can only manage roles below your highest role".into())
    }
    Ok(())
}

/// May `actor` change the roles of the owner (`target_is_owner`)? Only the owner.
pub fn can_touch(actor: &Actor, target_is_owner: bool) -> Result<(), String> {
    if target_is_owner && !actor.is_owner {
        return Err("you can't change the server owner's roles".into())
    }
    Ok(())
}

/// May `actor` assign the role at `position` to the target (`target_is_owner`)?
/// The owner gate first, then the hierarchy — the same pair the picker offers by.
pub fn can_assign(actor: &Actor, target_is_owner: bool, position: RolePosition) -> Result<(), String> {
    can_touch(actor, target_is_owner)
        .and_then(|_| can_give(actor, position))
}

#[cfg(test)]
mod tests {
    use crate::role_rules::{Actor, RolePosition, can_assign, can_give, can_touch};

    fn manager(highest: RolePosition) -> Actor {
        Actor { is_owner: false, manage_roles: true, highest_position: Some(highest) }
    }

    #[test]
    fn a_manager_hands_out_lower_roles_and_never_the_owners() {
        let bob = manager(3);
        assert!(can_give(&bob, 1).is_ok());
        assert!(can_give(&bob, 3).is_err());
        assert!(can_give(&bob, 5).is_err());
        assert!(can_touch(&bob, true).is_err());
        assert!(can_touch(&bob, false).is_ok());
    }

    #[test]
    fn the_owner_can_do_everything() {
        let owner = Actor { is_owner: true, manage_roles: false, highest_position: None };
        assert!(can_give(&owner, 5).is_ok());
        assert!(can_touch(&owner, true).is_ok());
        assert!(can_assign(&owner, true, 100).is_ok());
    }

    #[test]
    fn a_member_without_manage_roles_can_assign_nothing() {
        let plain = Actor { is_owner: false, manage_roles: false, highest_position: Some(2) };
        assert!(can_give(&plain, 1).is_err());
        assert!(can_assign(&plain, false, 1).is_err());
    }

    #[test]
    fn a_manager_without_roles_hands_out_nothing() {
        let none = Actor { is_owner: false, manage_roles: true, highest_position: None };
        assert!(can_give(&none, 0).is_err());
    }

    #[test]
    fn can_assign_agrees_with_can_give_and_can_touch() {
        let bob = manager(3);
        assert!(can_assign(&bob, false, 1).is_ok());
        assert!(can_assign(&bob, true, 1).is_err());
    }
}
