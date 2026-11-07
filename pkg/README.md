# Package manager recipes

Recipes for distributing TermTube outside GitHub releases. After tagging a release
(`vX.Y.Z`), run `scripts/update-packages.sh X.Y.Z` to point them at the new source
tarball, then publish:

## Homebrew (`brew install javierlongoriaroche/tap/termtube`)

The formula lives in the tap repository `javierlongoriaroche/homebrew-tap`:

```bash
cp pkg/homebrew/termtube.rb ../homebrew-tap/Formula/termtube.rb
cd ../homebrew-tap && git commit -am "termtube X.Y.Z" && git push
```

## crates.io (`cargo install termtube`)

```bash
cargo login          # once, with a token from https://crates.io/settings/tokens
cargo publish
```

## AUR (`yay -S termtube`)

Needs an AUR account with your SSH key (https://aur.archlinux.org):

```bash
git clone ssh://aur@aur.archlinux.org/termtube.git ../aur-termtube
cp pkg/aur/PKGBUILD pkg/aur/.SRCINFO ../aur-termtube/
cd ../aur-termtube && git add PKGBUILD .SRCINFO && git commit -m "termtube X.Y.Z" && git push
```

On Arch, regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO` if the PKGBUILD changes.
