use crate::types::Oid;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Role {
    pub rolname: String,
    pub rolsuper: bool,
    pub rolinherit: bool,
    pub rolcreaterole: bool,
    pub rolcreatedb: bool,
    pub rolcanlogin: bool,
    pub rolconnlimit: i32,
    pub rolpassword: Option<String>,
}

impl Role {
    pub fn new(name: &str) -> Self {
        Self {
            rolname: name.to_string(),
            rolsuper: false,
            rolinherit: true,
            rolcreaterole: false,
            rolcreatedb: false,
            rolcanlogin: true,
            rolconnlimit: -1,
            rolpassword: None,
        }
    }

    pub fn superuser(name: &str) -> Self {
        let mut role = Self::new(name);
        role.rolsuper = true;
        role.rolcreaterole = true;
        role.rolcreatedb = true;
        role
    }
}

pub const ROLE_POSTGRES: &str = "postgres";
pub const ROLE_PUBLIC: &str = "public";
pub const ROLE_PG_READ_ALL: &str = "pg_read_all_tables";
pub const ROLE_PG_WRITE_ALL: &str = "pg_write_all_tables";

#[derive(Debug, Clone)]
pub struct PrivilegeSet {
    pub select: bool,
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub truncate: bool,
    pub references: bool,
    pub trigger: bool,
}

impl PrivilegeSet {
    pub fn all() -> Self {
        Self {
            select: true,
            insert: true,
            update: true,
            delete: true,
            truncate: true,
            references: true,
            trigger: true,
        }
    }

    pub fn none() -> Self {
        Self {
            select: false,
            insert: false,
            update: false,
            delete: false,
            truncate: false,
            references: false,
            trigger: false,
        }
    }

    pub fn read() -> Self {
        Self {
            select: true,
            insert: false,
            update: false,
            delete: false,
            truncate: false,
            references: false,
            trigger: false,
        }
    }

    pub fn write() -> Self {
        Self {
            select: true,
            insert: true,
            update: true,
            delete: true,
            truncate: false,
            references: false,
            trigger: false,
        }
    }

    pub fn check(&self, required: &PrivilegeSet) -> bool {
        if required.select && !self.select { return false; }
        if required.insert && !self.insert { return false; }
        if required.update && !self.update { return false; }
        if required.delete && !self.delete { return false; }
        if required.truncate && !self.truncate { return false; }
        if required.references && !self.references { return false; }
        if required.trigger && !self.trigger { return false; }
        true
    }
}

#[derive(Debug, Clone)]
pub struct AccessControlList {
    pub grantee: Oid,
    pub grantor: Oid,
    pub rel_oid: Oid,
    pub privileges: PrivilegeSet,
}

impl AccessControlList {
    pub fn new(grantee: Oid, grantor: Oid, rel_oid: Oid, privileges: PrivilegeSet) -> Self {
        Self { grantee, grantor, rel_oid, privileges }
    }

    pub fn check(&self, required: &PrivilegeSet) -> bool {
        self.privileges.check(required)
    }
}

pub struct RbacManager {
    roles: HashMap<String, Role>,
    acls: Vec<AccessControlList>,
}

impl RbacManager {
    pub fn new() -> Self {
        let mut roles = HashMap::new();
        roles.insert(ROLE_POSTGRES.to_string(), Role::superuser(ROLE_POSTGRES));
        roles.insert(ROLE_PUBLIC.to_string(), {
            let mut r = Role::new(ROLE_PUBLIC);
            r.rolcanlogin = false;
            r
        });

        Self { roles, acls: Vec::new() }
    }

    pub fn create_role(&mut self, role: Role) -> anyhow::Result<()> {
        if self.roles.contains_key(&role.rolname) {
            anyhow::bail!("role \"{}\" already exists", role.rolname);
        }
        self.roles.insert(role.rolname.clone(), role);
        Ok(())
    }

    pub fn drop_role(&mut self, name: &str) -> anyhow::Result<()> {
        if name == ROLE_POSTGRES {
            anyhow::bail!("cannot drop role postgres");
        }
        self.roles.remove(name).ok_or_else(|| anyhow::anyhow!("role \"{}\" does not exist", name))?;
        Ok(())
    }

    pub fn get_role(&self, name: &str) -> Option<&Role> {
        self.roles.get(name)
    }

    pub fn list_roles(&self) -> Vec<&Role> {
        self.roles.values().collect()
    }

    pub fn grant(&mut self, grantee: Oid, rel_oid: Oid, privileges: PrivilegeSet) {
        let acl = AccessControlList::new(grantee, Oid(0), rel_oid, privileges);
        self.acls.push(acl);
    }

    pub fn revoke(&mut self, grantee: Oid, rel_oid: Oid, privileges: &PrivilegeSet) {
        self.acls.retain(|acl| {
            acl.grantee != grantee || acl.rel_oid != rel_oid
        });
    }

    pub fn check_privilege(&self, role_oid: Oid, rel_oid: Oid, required: &PrivilegeSet) -> bool {
        if let Some(role) = self.roles.values().find(|r| Oid(0) == role_oid) {
            if role.rolsuper {
                return true;
            }
        }

        self.acls.iter()
            .filter(|acl| acl.grantee == role_oid && acl.rel_oid == rel_oid)
            .any(|acl| acl.check(required))
    }
}

impl Default for RbacManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_new() {
        let role = Role::new("test_user");
        assert_eq!(role.rolname, "test_user");
        assert!(!role.rolsuper);
        assert!(role.rolcanlogin);
    }

    #[test]
    fn test_role_superuser() {
        let role = Role::superuser("admin");
        assert!(role.rolsuper);
        assert!(role.rolcreaterole);
        assert!(role.rolcreatedb);
    }

    #[test]
    fn test_privilege_set_all() {
        let all = PrivilegeSet::all();
        assert!(all.select);
        assert!(all.insert);
        assert!(all.update);
        assert!(all.delete);
    }

    #[test]
    fn test_privilege_set_check() {
        let read = PrivilegeSet::read();
        assert!(read.check(&PrivilegeSet::read()));
        assert!(!read.check(&PrivilegeSet::write()));
    }

    #[test]
    fn test_rbac_manager_create_role() {
        let mut mgr = RbacManager::new();
        let role = Role::new("test_user");
        mgr.create_role(role).unwrap();
        assert!(mgr.get_role("test_user").is_some());
    }

    #[test]
    fn test_rbac_manager_drop_role() {
        let mut mgr = RbacManager::new();
        mgr.create_role(Role::new("test_user")).unwrap();
        mgr.drop_role("test_user").unwrap();
        assert!(mgr.get_role("test_user").is_none());
    }

    #[test]
    fn test_rbac_manager_cannot_drop_postgres() {
        let mut mgr = RbacManager::new();
        assert!(mgr.drop_role("postgres").is_err());
    }

    #[test]
    fn test_grant_revoke() {
        let mut mgr = RbacManager::new();
        mgr.grant(Oid(1), Oid(100), PrivilegeSet::read());
        assert!(mgr.check_privilege(Oid(1), Oid(100), &PrivilegeSet::read()));
        mgr.revoke(Oid(1), Oid(100), &PrivilegeSet::read());
        assert!(!mgr.check_privilege(Oid(1), Oid(100), &PrivilegeSet::read()));
    }
}
