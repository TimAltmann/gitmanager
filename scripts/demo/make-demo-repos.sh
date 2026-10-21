#!/bin/bash
# Legt erfundene Demo-Repos unter $DEMO_HOME/Code an (Klone, Worktree,
# Branches, ahead/behind, Änderungen). Remotes zeigen auf
# gitlab.example.com, ohne Netz: Remote-Branches sind lokale Refs.
set -euo pipefail
CODE="${DEMO_HOME:?}/Code"
rm -rf "$CODE" && mkdir -p "$CODE"

export GIT_AUTHOR_NAME="Alex Berger" GIT_AUTHOR_EMAIL="alex.berger@example.com"
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
n=0
commit() { # commit <repo> <file> <message>
  n=$((n + 1))
  export GIT_AUTHOR_DATE="2026-10-0$(( (n % 9) + 1 ))T09:$(( n % 50 + 10 )):00Z"
  export GIT_COMMITTER_DATE="$GIT_AUTHOR_DATE"
  mkdir -p "$(dirname "$1/$2")" && echo "$3 ($n)" >> "$1/$2"
  git -C "$1" add -A && git -C "$1" commit -qm "$3"
}
remote_branch() { # remote_branch <repo> <branch> [<ref>]
  git -C "$1" update-ref "refs/remotes/origin/$2" "${3:-$2}"
}
track() { # track <repo> <branch>
  git -C "$1" config "branch.$2.remote" origin
  git -C "$1" config "branch.$2.merge" "refs/heads/$2"
}
new_repo() { # new_repo <path> <project> [<solution>]
  git init -q -b main "$1"
  git -C "$1" remote add origin "git@gitlab.example.com:acme/$2.git"
  echo "# $2" > "$1/README.md"
  [ -n "${3:-}" ] && printf 'Microsoft Visual Studio Solution File, Format Version 12.00\n' > "$1/$3"
  commit "$1" .gitignore "Initial commit"
  remote_branch "$1" main && track "$1" main
}

# shop-backend: Basis-Klon (main), zweiter Klon (eigener MR !128), Worktree (Review !134)
R="$CODE/shop-backend"
new_repo "$R" shop-backend ShopBackend.sln
commit "$R" src/Orders/OrderService.cs "Add order service"
commit "$R" src/Catalog/PriceList.cs "Add price list"
remote_branch "$R" main && track "$R" main
for b in feature/region-prices fix/null-currency chore/update-deps release/2.3; do
  git -C "$R" checkout -q -b "$b" main
  commit "$R" "src/${b//\//_}.cs" "Implement ${b#*/}"
  remote_branch "$R" "$b" && track "$R" "$b"
  git -C "$R" checkout -q main
done
git -C "$R" branch feature/checkout-redesign
remote_branch "$R" feature/checkout-redesign
git clone -q "$R" "$CODE/work/shop-backend"
W="$CODE/work/shop-backend"
git -C "$W" remote set-url origin "git@gitlab.example.com:acme/shop-backend.git"
git -C "$W" checkout -q feature/checkout-redesign 2>/dev/null || git -C "$W" checkout -q -b feature/checkout-redesign origin/feature/checkout-redesign
track "$W" feature/checkout-redesign
commit "$W" src/Checkout/CheckoutPage.cs "Redesign checkout summary"
commit "$W" src/Checkout/CheckoutPage.cs "Show delivery estimate"
git -C "$R" worktree add -q "$CODE/review/shop-backend-mr-134" feature/region-prices

# payment-service: main ist 3 Commits hinter origin/main
R="$CODE/payment-service"
new_repo "$R" payment-service PaymentService.sln
commit "$R" src/Webhooks/WebhookSender.cs "Add webhook sender"
git -C "$R" branch fix/webhook-retry
git -C "$R" checkout -q -b tmp-ahead
for m in "Retry webhooks with backoff" "Log provider response codes" "Bump SDK"; do commit "$R" src/Webhooks/Retry.cs "$m"; done
remote_branch "$R" main tmp-ahead && remote_branch "$R" fix/webhook-retry tmp-ahead
git -C "$R" checkout -q main && git -C "$R" branch -q -D tmp-ahead

# web-frontend: Feature-Branch mit offenen Änderungen
R="$CODE/web-frontend"
new_repo "$R" web-frontend
commit "$R" src/settings/Settings.tsx "Settings page"
git -C "$R" checkout -q -b feature/dark-mode
commit "$R" src/theme/dark.css "Dark palette"
remote_branch "$R" feature/dark-mode && track "$R" feature/dark-mode
for b in feature/date-picker chore/vite-7 fix/login-redirect; do git -C "$R" branch "$b"; remote_branch "$R" "$b"; done
echo "/* contrast tweaks */" >> "$R/src/theme/dark.css"
echo "export const accent = '#7aa2f7'" > "$R/src/theme/accent.ts"

# mobile-app: Release-Branch, 1 Commit voraus
R="$CODE/mobile-app"
new_repo "$R" mobile-app MobileApp.sln
git -C "$R" checkout -q -b release/2.4
remote_branch "$R" release/2.4 && track "$R" release/2.4
commit "$R" src/Orders/OfflineCache.cs "Cache order list offline"
git -C "$R" branch feature/offline-orders && remote_branch "$R" feature/offline-orders

# docs-site: develop
R="$CODE/docs-site"
new_repo "$R" docs-site
git -C "$R" checkout -q -b develop
commit "$R" docs/merge-requests.md "Document merge request view"
remote_branch "$R" develop && track "$R" develop
echo "demo repos: $(find "$CODE" -maxdepth 3 -name .git | wc -l)"
