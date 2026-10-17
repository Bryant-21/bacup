from pathlib import Path
import zipfile


FOMOD_XML = b'''<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
 xsi:noNamespaceSchemaLocation="http://qconsulting.ca/fo3/ModConfig5.0.xsd">
 <moduleName>Fixture texture pack</moduleName>
 <requiredInstallFiles><file source="base.txt" destination="textures/shared.txt" priority="0"/></requiredInstallFiles>
 <installSteps order="Explicit">
  <installStep name="Choose resolution"><optionalFileGroups order="Explicit">
   <group name="Resolution" type="SelectExactlyOne"><plugins order="Explicit">
    <plugin name="Standard"><description>A small texture set.</description>
     <files><folder source="standard" destination="textures" priority="1"/></files>
     <conditionFlags><flag name="quality">standard</flag></conditionFlags>
     <typeDescriptor><type name="Recommended"/></typeDescriptor></plugin>
    <plugin name="High"><description>A larger texture set.</description>
     <files><folder source="high" destination="textures" priority="2"/></files>
     <conditionFlags><flag name="quality">high</flag></conditionFlags>
     <typeDescriptor><type name="Optional"/></typeDescriptor></plugin>
   </plugins></group></optionalFileGroups></installStep>
  <installStep name="Optional compatibility"><visible operator="And"><flagDependency flag="quality" value="high"/></visible>
   <optionalFileGroups><group name="Extras" type="SelectAny"><plugins>
    <plugin name="Detail"><files><file source="detail.txt" destination="textures/detail.txt"/></files>
     <typeDescriptor><dependencyType><defaultType name="Optional"/><patterns>
      <pattern><dependencies><fileDependency file="Fixture.esp" state="Active"/></dependencies><type name="Required"/></pattern>
     </patterns></dependencyType></typeDescriptor></plugin>
   </plugins></group></optionalFileGroups></installStep>
 </installSteps>
 <conditionalFileInstalls><patterns>
  <pattern><dependencies><flagDependency flag="quality" value="high"/></dependencies>
   <files><file source="patch.txt" destination="textures/shared.txt" priority="10"/></files></pattern>
  <pattern><dependencies operator="Or"><fileDependency file="Absent.esp" state="Missing"/>
   <dependencies operator="And"><flagDependency flag="quality" value="high"/><fileDependency file="Fixture.esp" state="Active"/></dependencies>
   </dependencies><files><file source="readme.txt" destination="readme.txt"/></files></pattern>
 </patterns></conditionalFileInstalls>
</config>'''


def make_package(directory: Path, *, zipped=True):
    files = {"fomod/ModuleConfig.xml": FOMOD_XML, "base.txt": b"base", "standard/shared.txt": b"standard",
             "high/shared.txt": b"high", "high/extra.txt": b"extra", "detail.txt": b"detail",
             "patch.txt": b"patch", "readme.txt": b"readme"}
    if zipped:
        path = directory / "FixtureMod.zip"
        with zipfile.ZipFile(path, "w") as archive:
            for name, data in files.items():
                archive.writestr(name, data)
        return path
    path = directory / "package"
    for name, data in files.items():
        output = path / name
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(data)
    return path


def make_mo2(directory: Path):
    root = directory / "fixture-mo2"
    root.mkdir()
    (root / "ModOrganizer.ini").write_text('[General]\ngameName=Fallout 4\n[Settings]\nbase_directory=' + str(root) + '\nmod_directory=%BASE_DIR%/mods\nprofiles_directory=%BASE_DIR%/profiles\n', encoding="utf-8")
    (root / "categories.dat").write_text("1|Visuals||0\n", encoding="utf-8")
    for name, data in (("Base", b"base"), ("Override", b"override"), ("Disabled", b"disabled")):
        folder = root / "mods" / name
        (folder / "textures").mkdir(parents=True)
        (folder / "textures" / "shared.txt").write_bytes(data)
        (folder / "meta.ini").write_text("[General]\nversion=1.2\ncategory=1\nnotes=fixture\n", encoding="utf-8")
        (folder / "Fixture.esp").write_bytes(b"synthetic fixture; not a game plugin")
        (folder / "hidden.mohidden").write_bytes(b"hidden")
    (root / "mods" / "Visual_separator").mkdir()
    for name in ("Default", "Alternate"):
        profile = root / "profiles" / name
        profile.mkdir(parents=True)
        (profile / "modlist.txt").write_text("# MO2 reverse order\n-Disabled\n+Override\n+Base\n-Visual_separator\n", encoding="utf-8")
        (profile / "plugins.txt").write_text("*Fixture.esp\nDisabled.esp\n", encoding="utf-8")
        (profile / "loadorder.txt").write_text("Fixture.esp\nDisabled.esp\n", encoding="utf-8")
        (profile / "Fallout4.ini").write_text("[Fixture]\nvalue=unchanged\n", encoding="utf-8")
    return root
