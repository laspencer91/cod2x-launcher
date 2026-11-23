# Publish a mod

Each mod uses its own public GitHub repository. Once the catalog lists it, anyone with release access to that repository can publish updates. Players can browse and install without signing in.

## Prepare a release

1. In the launcher, open **Other Mods → Publish a mod**.
2. For an existing mod, choose its public repository or paste its link. The form loads its latest package details and suggests the next version. For a new mod, enter the name, credits, and description. You can leave the repository blank and use **Copy publishing prompt** to have your agent create it.
3. Choose the build output folder. Select only the files you intend to ship. The scanner suggests a custom **mss32.dll** and mod **.iwd** files inside **main/**. Official archives and a recognized official DLL are excluded. Use the build output so other mods and downloaded maps are easy to avoid.
4. **Copy publishing prompt** delegates packaging, publishing, verification, and a first catalog request to your coding agent. It works from the release details, build files, or exported package. For manual publishing, **Export release files** creates **mod.zip** and **mod.json** in a versioned output folder. The descriptor contains the package metadata, size, and SHA-256.
5. **Open GitHub release draft** takes you to the mod repo’s Releases form. Use tag **v<version>** and attach **both** assets with their exact filenames. Publish the release through GitHub.
6. For a new mod, click **Submit for review** with your GitHub account connected, or choose **Use GitHub form**. The catalog checks your public release and posts a pull request link on your listing request. A maintainer reviews the entry. Their approval lets GitHub merge it automatically. [Catalog listing form](https://github.com/laspencer91/cod2x-mod-catalog/issues/new?template=publisher-request.yml)

Already published a compatible release? Select its repository in the first step and click **Submit for review** there. The launcher can list the existing release.

After the first listing, publish subsequent releases directly on GitHub. Use the same repository and mod ID, a fresh **v<version>** tag, and matching **mod.zip** and **mod.json** assets. The launcher checks the catalog on startup and every five minutes while visible. Players choose when to install an update.

![Publishing an update from an existing mod repository](publisher.png)

## Repository layout

A first release can start with a README as the only file on the default branch. When the prompt has an empty repository field, check the mod project and catalog for an existing home for the same mod ID. For a new project, create a dedicated public repository under the signed-in publisher’s account, using the mod ID as its name. Check the proposed name first and resolve any collision with an unrelated project.

```powershell
$owner = gh api user --jq .login
$repo = "$owner/my-mod"  # Replace my-mod with the stable mod ID from the release brief.
gh repo create $repo --public --add-readme
gh repo view $repo --json nameWithOwner,visibility,viewerPermission,defaultBranchRef
```

Run the creation command once. On a retry, reuse the repository you created. Keep the game installation local. Upload the two package files as assets on the GitHub release.

A repo just needs a default branch and published releases. The branch can contain your source, a README, or both. A public release uses **v<version>** as the tag and includes **mod.zip** and **mod.json** as assets. The launcher checks that the descriptor matches the approved mod ID, tag, archive size, and GitHub release digest.

A catalog entry is **{"id":"your-mod-id","repository":"owner/your-mod-repo"}** in the **mods** array of the catalog’s **catalog.json**. Forks of the launcher can choose their own registry using **COD2X_MODS_REPOSITORY** at build time. The default is the public CoD2x Mod Catalog linked above.

## Use the same flow from a script

**Copy publishing prompt** includes the release metadata, chosen repository, build folder, selected file hashes, and any exported assets. You can also run the package helper through PowerShell or give these commands to a coding agent.

```powershell
$build = @{
    Source       = 'C:\Builds\MyMod'
    Id           = 'my-mod'
    Name         = 'My Mod'
    Version      = '0.1.0'
    BaseVersion  = '1.4.6.8'
    Description  = 'A short description of the build.'
    Authors      = @('Your public name')
    Kind         = 'addon'
    Files        = @('main/my-mod.iwd')
    ReleaseNotes = 'Initial release.'
    ReleaseAssets = $true
    Output       = 'C:\Builds\Release\mod.zip'
}
.\tools\New-ModPackage.ps1 @build
```

For a custom client, use **Kind = 'runtime'** and add **mss32.dll** to **Files**. Create the output directory first. The script writes **mod.json** next to the ZIP. It prints the JSON descriptor for automation.

With an authenticated GitHub CLI, publish the two assets together. Use the release notes from the publisher’s brief.

```powershell
gh release create v0.1.0 C:\Builds\Release\mod.zip C:\Builds\Release\mod.json --repo owner/my-mod --title 'My Mod 0.1.0' --notes-file release-notes.md
gh release view v0.1.0 --repo owner/my-mod --json url,isDraft,tagName,assets
```

Check the two GitHub asset digests against the local hashes. Reuse an identical published tag on a retry. A conflicting tag requires a new version. An update to an existing listing needs only the new release.

For a first listing, check the catalog and open requests for the same mod. Create a temporary `listing.md` with the following headings and replace the sample values.

```markdown
### Mod ID

my-mod

### Public mod repository

https://github.com/owner/my-mod
```

Run `gh issue create --repo laspencer91/cod2x-mod-catalog --title "List my-mod" --body-file listing.md`. The catalog workflow reads the fields, checks the release, and prepares a pull request that adds the entry to `catalog.json`. Read the issue comments for the review link or validation feedback. Return the release and review URLs to the publisher. The catalog maintainer supplies the approval.
