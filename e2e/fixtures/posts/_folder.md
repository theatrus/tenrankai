+++
[permissions]
public_role = "viewer"

[permissions.roles.viewer]
name = "viewer"
permissions = { can_view = true }

[permissions.roles.editor]
name = "editor"
permissions = { can_view = true, can_edit_content = true }

[[permissions.user_roles]]
username = "e2euser"
roles = ["editor"]
+++
