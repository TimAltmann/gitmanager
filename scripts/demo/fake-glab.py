#!/usr/bin/env python3
"""Fake-`glab` für Screenshots und Videos: beantwortet die Aufrufe von
GitManager (`version`, `config path|get`, `auth status`, `api …`) mit
erfundenen Daten für gitlab.example.com. Kein Netz, keine echten Daten.
Zeitangaben sind relativ zu jetzt. Aufrufe landen in $FAKE_GLAB_LOG."""

import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone

HOST = "gitlab.example.com"
ME = ("aberger", "Alex Berger")
PEOPLE = {
    "mkeller": "Mia Keller",
    "jweber": "Jonas Weber",
    "snovak": "Sara Novak",
    "lbrandt": "Lukas Brandt",
    "dchen": "David Chen",
    "efischer": "Emma Fischer",
    "renovate-bot": "Renovate Bot",
    ME[0]: ME[1],
}
NOW = datetime.now(timezone.utc)


def ago(minutes):
    return (NOW - timedelta(minutes=minutes)).strftime("%Y-%m-%dT%H:%M:%SZ")


def user(username):
    return {"username": username, "name": PEOPLE[username], "bot": username.endswith("-bot")}


def mr(iid, project, title, author, branch, minutes, target="main", approved=(),
       required=2, reviewers=(), pipeline="SUCCESS", threads=(0, 0), status=None,
       draft=False):
    """MR-Knoten wie das Fragment `Mr`; reviewers = [(username, reviewState)]."""
    path = f"acme/{project}"
    open_, resolved = threads
    left = max(required - len(approved), 0)
    if status is None:
        status = "DRAFT_STATUS" if draft else ("MERGEABLE" if left == 0 and open_ == 0
                                               and pipeline == "SUCCESS" else "NOT_APPROVED")
    pipe_id = 4000 + iid * 7
    return {
        "id": f"gid://gitlab/MergeRequest/{90000 + iid}",
        "iid": str(iid),
        "title": title,
        "draft": draft,
        "webUrl": f"https://{HOST}/{path}/-/merge_requests/{iid}",
        "webPath": f"/{path}/-/merge_requests/{iid}",
        "updatedAt": ago(minutes),
        "sourceBranch": branch,
        "targetBranch": target,
        "sourceBranchExists": True,
        "diffHeadSha": f"{iid:040x}",
        "project": {"fullPath": path},
        "sourceProject": {"fullPath": path},
        "author": {"username": author, "name": PEOPLE[author]},
        "approvedBy": {"nodes": [user(u) for u in approved]},
        "approvalsRequired": required,
        "approvalsLeft": left,
        "headPipeline": None if pipeline is None else {
            "status": pipeline,
            "path": f"/{path}/-/pipelines/{pipe_id}",
            "detailedStatus": {"detailsPath": f"/{path}/-/pipelines/{pipe_id}"},
        },
        "resolvableDiscussionsCount": open_ + resolved,
        "resolvedDiscussionsCount": resolved,
        "detailedMergeStatus": status,
        "reviewers": {"nodes": [
            {"username": u, "name": PEOPLE[u], "mergeRequestInteraction": {"reviewState": s}}
            for u, s in reviewers
        ]},
    }


MINE = [
    mr(131, "payment-service", "Retry failed webhooks with exponential backoff", ME[0],
       "fix/webhook-retry", 25, approved=["mkeller", "dchen"],
       reviewers=[("mkeller", "APPROVED"), ("dchen", "APPROVED")], threads=(0, 4)),
    mr(128, "shop-backend", "Redesign checkout summary and delivery estimate", ME[0],
       "feature/checkout-redesign", 12, approved=["mkeller"],
       reviewers=[("mkeller", "APPROVED"), ("jweber", "REVIEW_STARTED")], threads=(2, 3)),
    mr(19, "docs-site", "Document the merge request view", ME[0], "docs/merge-requests",
       180, target="develop", approved=["efischer"], required=0,
       reviewers=[("efischer", "APPROVED")], pipeline=None),
    mr(57, "web-frontend", "Dark mode for the settings page", ME[0], "feature/dark-mode",
       45, approved=[], reviewers=[("snovak", "UNREVIEWED")], pipeline="RUNNING",
       draft=True),
]
REVIEWING = [
    mr(134, "shop-backend", "Cache product prices per region", "jweber",
       "feature/region-prices", 8, reviewers=[(ME[0], "UNREVIEWED"), ("mkeller", "UNREVIEWED")],
       threads=(1, 0)),
    mr(88, "web-frontend", "Replace the date picker component", "snovak",
       "feature/date-picker", 95, approved=["dchen"],
       reviewers=[(ME[0], "REVIEW_STARTED"), ("dchen", "APPROVED")], pipeline="FAILED",
       threads=(3, 1)),
    mr(12, "payment-service", "Support partial refunds", "dchen", "feature/partial-refunds",
       300, reviewers=[(ME[0], "REQUESTED_CHANGES")], threads=(2, 2)),
    mr(133, "shop-backend", "Upgrade to .NET 10", "mkeller", "chore/dotnet-10", 60,
       approved=[ME[0]], reviewers=[(ME[0], "APPROVED"), ("jweber", "UNREVIEWED")]),
]
# (MR, Erwähnung in der Beschreibung?)
MENTIONS = [
    (mr(42, "mobile-app", "Offline mode for the order list", "lbrandt",
        "feature/offline-orders", 140, target="release/2.4", approved=["snovak"],
        threads=(1, 2)), True),
    (mr(90, "web-frontend", "Update dependency vite to v7", "renovate-bot", "chore/vite-7",
        30, approved=["snovak", "dchen", ME[0]], pipeline="SUCCESS"), False),
]
UNINVOLVED = {
    "acme/shop-backend": [
        mr(136, "shop-backend", "Fix rounding of discount totals", "efischer",
           "fix/null-currency", 20, approved=["jweber"], pipeline="SUCCESS"),
    ],
    "acme/web-frontend": [
        mr(91, "web-frontend", "Fix login redirect after timeout", "lbrandt",
           "fix/login-redirect", 70, pipeline="PENDING"),
    ],
    "acme/docs-site": [
        mr(21, "docs-site", "Add FAQ for SSH keys", "efischer", "docs/ssh-faq", 400,
           target="develop", required=0, pipeline=None),
    ],
}
MERGED = [
    (mr(125, "shop-backend", "Add order export as CSV", ME[0], "feature/csv-export", 1500,
        approved=["mkeller", "jweber"]), 1440, "mkeller"),
    (mr(127, "payment-service", "Validate IBAN before payout", ME[0], "fix/iban-check", 3200,
        approved=["dchen", "mkeller"]), 3100, "dchen"),
    (mr(55, "web-frontend", "Keyboard shortcuts for the cart", ME[0], "feature/shortcuts",
        7000, approved=["snovak", "lbrandt"]), 6900, ME[0]),
    (mr(17, "docs-site", "Getting started guide", ME[0], "docs/getting-started", 12000,
        target="develop", approved=["efischer"], required=0, pipeline=None), 11900, "efischer"),
    (mr(40, "mobile-app", "Push notifications for shipped orders", ME[0],
        "feature/push-shipped", 20000, target="release/2.4", approved=["lbrandt", "snovak"]),
     19900, "lbrandt"),
]
PROJECTS = ["acme/shop-backend", "acme/payment-service", "acme/web-frontend",
            "acme/mobile-app", "acme/docs-site"]
NO_MORE = {"hasNextPage": False, "endCursor": None}
METADATA = {"version": "18.5.1-ee", "enterprise": True}


def graphql(request):
    query = request.get("query", "")
    variables = request.get("variables") or {}
    op = (re.search(r"query (Gm\w+)", query) or [None, ""])[1]
    if op == "GmDashboard":
        return {"data": {"metadata": METADATA, "currentUser": {
            "username": ME[0], "name": ME[1],
            "authored": {"pageInfo": NO_MORE, "nodes": MINE},
            "reviewing": {"pageInfo": NO_MORE, "nodes": REVIEWING},
        }}}
    if op == "GmMentions":
        todos = [{
            "id": f"gid://gitlab/Todo/{7000 + i}",
            "action": "mentioned",
            "note": None if in_description else {"id": f"gid://gitlab/Note/{500 + i}"},
            "targetEntity": dict(node, state="opened"),
        } for i, (node, in_description) in enumerate(MENTIONS)]
        return {"data": {"currentUser": {"username": ME[0],
                                         "todos": {"pageInfo": NO_MORE, "nodes": todos}}}}
    if op == "GmUninvolved":
        paths = variables.get("paths") or PROJECTS
        nodes = []
        for path in paths:
            mrs = UNINVOLVED.get(path.lower(), [])
            nodes.append({"fullPath": path, "mergeRequests": {
                "count": len(mrs), "pageInfo": NO_MORE, "nodes": mrs}})
        return {"data": {"projects": {"nodes": nodes}}}
    if op == "GmMerged":
        since = variables.get("since") or ""
        nodes = [dict(node, mergedAt=ago(at), mergeUser={"username": by, "name": PEOPLE[by]})
                 for node, at, by in MERGED if ago(at) >= since]
        return {"data": {"currentUser": {"username": ME[0], "authoredMergeRequests": {
            "count": len(nodes), "pageInfo": NO_MORE, "nodes": nodes}}}}
    if op == "GmAccess":
        paths = variables.get("paths") or []
        return {"data": {
            "currentUser": {"username": ME[0], "name": ME[1], "bot": False,
                            "todos": {"pageInfo": NO_MORE,
                                      "nodes": [{"id": "gid://gitlab/Todo/1"}]}},
            "metadata": METADATA,
            "projects": {"pageInfo": NO_MORE, "nodes": [{
                "fullPath": p, "webUrl": f"https://{HOST}/{p}", "archived": False,
                "mergeRequestsEnabled": True,
                "maxAccessLevel": {"integerValue": 30, "stringValue": "DEVELOPER"},
                "userPermissions": {"readMergeRequest": True},
            } for p in paths]},
        }}
    return {"errors": [{"message": f"fake-glab: unknown query {op or '?'}"}]}


def rest(endpoint):
    if endpoint == "user":
        return {"id": 42, "username": ME[0], "name": ME[1], "bot": False, "state": "active",
                "web_url": f"https://{HOST}/{ME[0]}"}
    if endpoint == "personal_access_tokens/self":
        return {"id": 4711, "name": "GitManager", "revoked": False,
                "created_at": "2026-09-01T08:00:00.000Z", "description": "",
                "scopes": ["api", "write_repository"], "user_id": 42, "last_used_at": ago(5),
                "last_used_ips": [], "active": True, "granular": False,
                "expires_at": "2027-09-01"}
    if endpoint.startswith("projects/"):
        path = endpoint[len("projects/"):].replace("%2F", "/").replace("%2f", "/")
        return {"id": 100 + PROJECTS.index(path) if path in PROJECTS else 999,
                "path_with_namespace": path, "web_url": f"https://{HOST}/{path}",
                "ssh_url_to_repo": f"git@{HOST}:{path}.git",
                "http_url_to_repo": f"https://{HOST}/{path}.git",
                "archived": False, "merge_requests_enabled": True,
                "permissions": {"project_access": {"access_level": 30}}}
    return None


def http(body, code=200, reason="OK"):
    sys.stdout.write(f"HTTP/2.0 {code} {reason}\r\nContent-Type: application/json\r\n\r\n")
    sys.stdout.write(json.dumps(body))


def main(args):
    if args[:1] == ["version"]:
        print("glab 1.123.0 (2026-09-15)")
        return 0
    if args[:2] == ["config", "path"]:
        print(os.path.join(os.environ["GLAB_CONFIG_DIR"], "config.yml"))
        return 0
    if args[:2] == ["config", "get"]:
        key = args[-1]
        print({"user": ME[0], "api_protocol": "https", "git_protocol": "ssh",
               "use_keyring": "true"}.get(key, ""))
        return 0
    if args[:2] == ["auth", "status"]:
        print(json.dumps({"hosts": [{
            "host": HOST, "state": "success", "user": ME[0], "error": "",
            "token_source": "keyring", "git_protocol": "ssh", "api_protocol": "https",
            "api_endpoint": f"https://{HOST}/api/v4/",
            "graphql_endpoint": f"https://{HOST}/api/graphql/", "subfolder": "",
            "ssh_host": "", "warnings": []}]}))
        return 0
    if args[:1] == ["api"]:
        rest_args = [a for a in args[1:] if not a.startswith("-")]
        # --hostname H, -X POST, -H "…", --input - → Werte überspringen
        skip = {"--hostname", "-X", "-H", "--input"}
        positional = []
        it = iter(args[1:])
        for a in it:
            if a in skip:
                next(it, None)
            elif not a.startswith("-"):
                positional.append(a)
        endpoint = positional[0] if positional else (rest_args[0] if rest_args else "")
        if endpoint == "graphql":
            body = graphql(json.loads(sys.stdin.read() or "{}"))
            http(body)
            return 1 if "errors" in body and "data" not in body else 0
        body = rest(endpoint)
        if body is None:
            http({"message": "404 Not Found"}, 404, "Not Found")
            return 1
        http(body)
        return 0
    sys.stderr.write(f"fake-glab: unsupported {' '.join(args)}\n")
    return 1


if __name__ == "__main__":
    log = os.environ.get("FAKE_GLAB_LOG")
    if log:
        with open(log, "a", encoding="utf-8") as f:
            f.write(json.dumps(sys.argv[1:]) + "\n")
    sys.exit(main(sys.argv[1:]))
