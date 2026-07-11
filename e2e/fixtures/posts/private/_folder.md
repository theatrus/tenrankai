+++
[permissions]
public_role = "none"
default_authenticated_role = "none"

[permissions.roles.owner]
name = "owner"
permissions = { can_view = true, can_edit_content = true }

[[permissions.user_roles]]
username = "e2euser"
roles = ["owner"]
+++
