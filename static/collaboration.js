const $ = (id) => document.getElementById(id);
let currentGroup = "";

async function api(path, options = {}) {
  const response = await fetch(path, { headers: { "Content-Type": "application/json", ...(options.headers || {}) }, ...options });
  if (!response.ok) throw new Error((await response.text()) || `${response.status}`);
  const type = response.headers.get("content-type") || "";
  return type.includes("json") ? response.json() : response.text();
}
function body(form) { return JSON.stringify(Object.fromEntries(new FormData(form))); }
function escapeHtml(value) { const node = document.createElement("span"); node.textContent = String(value ?? ""); return node.innerHTML; }
function fail(error) { $("error").textContent = error.message || String(error); }

async function refresh() {
  $("error").textContent = "";
  const [{ agents }, { groups }] = await Promise.all([api("/api/collaboration/agents"), api("/api/collaboration/groups")]);
  $("agents").innerHTML = agents.map((a) => `<div class="collab-item"><strong>${escapeHtml(a.name)}</strong><div class="muted">${escapeHtml(a.agent_id)} · ${escapeHtml(a.platform)} · ${escapeHtml(a.status)}</div></div>`).join("");
  $("groups").innerHTML = groups.map((g) => `<button class="collab-item ${g.group_id === currentGroup ? "active" : ""}" data-group="${escapeHtml(g.group_id)}"><strong>${escapeHtml(g.name)}</strong><div class="muted">${escapeHtml(g.project_id)} · ${g.members.length} 人 · #${g.last_sequence}</div></button>`).join("");
  $("groups").querySelectorAll("[data-group]").forEach((button) => button.addEventListener("click", () => openGroup(button.dataset.group)));
  if (currentGroup) await openGroup(currentGroup, false);
}

async function openGroup(groupId, rerender = true) {
  currentGroup = groupId;
  const [{ group, markdown_path }, { messages }, markdown] = await Promise.all([
    api(`/api/collaboration/groups/${groupId}`),
    api(`/api/collaboration/groups/${groupId}/messages?limit=500`),
    api(`/api/collaboration/groups/${groupId}/markdown`),
  ]);
  $("group-title").textContent = group.name;
  $("group-meta").textContent = `${group.topic || "无主题"} · ${group.members.map((m) => `${m.agent_id}:${m.role}`).join(", ")} · ${markdown_path}`;
  $("messages").innerHTML = messages.map((m) => `<article class="message"><strong>#${m.sequence} ${escapeHtml(m.sender_agent_id)}</strong> <span class="muted">${escapeHtml(m.kind)} · ${new Date(m.created_at * 1000).toLocaleString()}</span><div>${escapeHtml(m.text).replaceAll("\n", "<br>")}</div><div class="muted">${escapeHtml(JSON.stringify(m.acknowledgements))}</div></article>`).join("");
  $("markdown").textContent = markdown;
  $("join-form").hidden = false;
  $("ack-form").hidden = false;
  $("message-form").hidden = false;
  if (rerender) await refresh();
}

$("agent-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api("/api/collaboration/agents", { method: "POST", body: body(event.currentTarget) }); event.currentTarget.reset(); await refresh(); } catch (error) { fail(error); } });
$("group-form").addEventListener("submit", async (event) => { event.preventDefault(); try { const group = await api("/api/collaboration/groups", { method: "POST", body: body(event.currentTarget) }); event.currentTarget.reset(); currentGroup = group.group_id; await refresh(); } catch (error) { fail(error); } });
$("join-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api(`/api/collaboration/groups/${currentGroup}/join`, { method: "POST", body: body(event.currentTarget) }); await openGroup(currentGroup); } catch (error) { fail(error); } });
$("ack-form").addEventListener("submit", async (event) => { event.preventDefault(); try { const values = Object.fromEntries(new FormData(event.currentTarget)); const messageId = values.message_id; delete values.message_id; await api(`/api/collaboration/messages/${messageId}/ack`, { method: "POST", body: JSON.stringify(values) }); await openGroup(currentGroup); } catch (error) { fail(error); } });
$("message-form").addEventListener("submit", async (event) => { event.preventDefault(); try { await api(`/api/collaboration/groups/${currentGroup}/messages`, { method: "POST", body: body(event.currentTarget) }); event.currentTarget.querySelector("textarea").value = ""; await openGroup(currentGroup); } catch (error) { fail(error); } });
$("refresh").addEventListener("click", () => refresh().catch(fail));
$("reconcile").addEventListener("click", async () => { try { await api(`/api/collaboration/groups/${currentGroup}/reconcile`, { method: "POST" }); await openGroup(currentGroup); } catch (error) { fail(error); } });
refresh().catch(fail);
