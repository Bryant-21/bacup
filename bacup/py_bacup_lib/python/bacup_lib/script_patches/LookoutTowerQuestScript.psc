; FO76 filled MapMarkersToReveal with ALFF -- a scope-free ref-type match
; over MapMarkerRefType, narrowed by a 40000-unit distance condition.
; FO4 has no equivalent alias fill: ALRT is only legal paired with ALFA
; against a Location alias, which is location-scoped rather than radial.
; The radius rule therefore runs here. The baked list is only a candidate
; set (every marker within reach of some lookout tower); the 40000-unit
; test below is what decides what a given tower actually reveals.

Event OnQuestInit()
    Actor surveyor = Game.GetPlayer()
    Int revealedCount = 0

    If surveyor != None
        revealedCount += RevealBatch00(surveyor)
        revealedCount += RevealBatch01(surveyor)
        revealedCount += RevealBatch02(surveyor)
        revealedCount += RevealBatch03(surveyor)
        revealedCount += RevealBatch04(surveyor)
        revealedCount += RevealBatch05(surveyor)
    EndIf

    If LookoutTowerSurveyMessage != None
        LookoutTowerSurveyMessage.Show(revealedCount)
    EndIf

    Stop()
EndEvent

Int Function RevealIfNear(Actor akSurveyor, Int aiMarkerID)
    ObjectReference marker = Game.GetFormFromFile(aiMarkerID, "SeventySix.esm") as ObjectReference
    If marker == None || marker.IsMapMarkerVisible()
        Return 0
    EndIf
    If marker.GetDistance(akSurveyor) > 40000.0
        Return 0
    EndIf
    marker.AddToMap(False)
    If marker.IsMapMarkerVisible()
        Return 1
    EndIf
    Return 0
EndFunction

Int Function RevealBatch00(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x0000303A) ; National Isolated Radio Array
    found += RevealIfNear(akSurveyor, 0x00004152) ; Abandoned Bog Town
    found += RevealIfNear(akSurveyor, 0x000041B4) ; Poseidon Energy Plant WV-06
    found += RevealIfNear(akSurveyor, 0x00005067) ; Kanawha Nuka-Cola Plant
    found += RevealIfNear(akSurveyor, 0x00005069) ; The General's Steakhouse
    found += RevealIfNear(akSurveyor, 0x00006D97) ; Watoga
    found += RevealIfNear(akSurveyor, 0x0000B14F) ; Hopewell Cave
    found += RevealIfNear(akSurveyor, 0x0000FBC0) ; Vault-Tec Agricultural Research Center
    found += RevealIfNear(akSurveyor, 0x00015553) ; Flatwoods
    found += RevealIfNear(akSurveyor, 0x00018DD9) ; Overlook Cabin
    found += RevealIfNear(akSurveyor, 0x00018EC0) ; Camden Park
    found += RevealIfNear(akSurveyor, 0x00019230) ; Pleasant Valley Ski Resort
    found += RevealIfNear(akSurveyor, 0x000193A0) ; Pleasant Valley Cabins
    found += RevealIfNear(akSurveyor, 0x000193A3) ; Top of the World
    found += RevealIfNear(akSurveyor, 0x000193CA) ; Riverside Manor
    found += RevealIfNear(akSurveyor, 0x0001BAB6) ; East Kanawha Lookout
    found += RevealIfNear(akSurveyor, 0x0001BABA) ; Central Mountain Lookout
    found += RevealIfNear(akSurveyor, 0x0001BABE) ; North Mountain Lookout
    found += RevealIfNear(akSurveyor, 0x000537D3) ; Dyer Chemical
    found += RevealIfNear(akSurveyor, 0x00058490) ; Haven Church
    found += RevealIfNear(akSurveyor, 0x0005AD20) ; Devil's Backbone
    found += RevealIfNear(akSurveyor, 0x0005AD71) ; Dolly Sods Wilderness
    found += RevealIfNear(akSurveyor, 0x0005AED4) ; Lewis & Sons Farming Supply
    found += RevealIfNear(akSurveyor, 0x0005CC39) ; Clarksburg
    found += RevealIfNear(akSurveyor, 0x0005D32C) ; Billings Homestead
    found += RevealIfNear(akSurveyor, 0x0005D339) ; Helvetia
    found += RevealIfNear(akSurveyor, 0x0005F072) ; Clarksburg Shooting Club
    found += RevealIfNear(akSurveyor, 0x0005F080) ; Cow Spots Creamery
    found += RevealIfNear(akSurveyor, 0x0005F801) ; Kerwood Mine
    found += RevealIfNear(akSurveyor, 0x00060CC8) ; Berkeley Springs
    found += RevealIfNear(akSurveyor, 0x000616B5) ; Lucky Hole Mine
    found += RevealIfNear(akSurveyor, 0x000617B9) ; Freddy Fear's House of Scares
    found += RevealIfNear(akSurveyor, 0x00063D80) ; Gorge Junkyard
    found += RevealIfNear(akSurveyor, 0x0006D2FD) ; Grafton Steel
    found += RevealIfNear(akSurveyor, 0x0006DE28) ; Grafton
    found += RevealIfNear(akSurveyor, 0x0006DE3A) ; National Radio Research Center
    found += RevealIfNear(akSurveyor, 0x00071A81) ; Hillfolk Hotdogs
    found += RevealIfNear(akSurveyor, 0x00071AA2) ; Tygart Water Treatment
    found += RevealIfNear(akSurveyor, 0x00071AC5) ; Hunter's Shack
    found += RevealIfNear(akSurveyor, 0x0007201E) ; Huntersville
    found += RevealIfNear(akSurveyor, 0x00072601) ; Big Fred's BBQ Shack
    found += RevealIfNear(akSurveyor, 0x00072606) ; Isolated Cabin
    found += RevealIfNear(akSurveyor, 0x00072608) ; Watoga Shopping Plaza
    found += RevealIfNear(akSurveyor, 0x0007260B) ; AMS Corporate Headquarters
    found += RevealIfNear(akSurveyor, 0x00072616) ; Watoga Civic Center
    Return found
EndFunction

Int Function RevealBatch01(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x00081FC4) ; Bootlegger's Shack
    found += RevealIfNear(akSurveyor, 0x00083A7A) ; Relay Tower EL-B1-02
    found += RevealIfNear(akSurveyor, 0x000863F2) ; AVR Medical Center
    found += RevealIfNear(akSurveyor, 0x0008CCC9) ; Morgantown Airport
    found += RevealIfNear(akSurveyor, 0x0008DFF8) ; Eastern Regional Penitentiary
    found += RevealIfNear(akSurveyor, 0x0008EDB2) ; Lakeside Cabins
    found += RevealIfNear(akSurveyor, 0x00090576) ; Uncanny Caverns
    found += RevealIfNear(akSurveyor, 0x00090626) ; Sunshine Meadows Industrial Farm
    found += RevealIfNear(akSurveyor, 0x00090652) ; Green Country Lodge
    found += RevealIfNear(akSurveyor, 0x00091F06) ; Greg's Mine Supply
    found += RevealIfNear(akSurveyor, 0x00092028) ; Miners Monument
    found += RevealIfNear(akSurveyor, 0x00093ADF) ; Colonel Kelley Monument
    found += RevealIfNear(akSurveyor, 0x0009530B) ; Alpine River Cabins
    found += RevealIfNear(akSurveyor, 0x00095313) ; Mountainside Bed & Breakfast
    found += RevealIfNear(akSurveyor, 0x000955B4) ; Thunder Mountain Power Plant
    found += RevealIfNear(akSurveyor, 0x000969F1) ; Philippi Battlefield Cemetery
    found += RevealIfNear(akSurveyor, 0x00096A01) ; New River Gorge Resort
    found += RevealIfNear(akSurveyor, 0x00096AAB) ; Orwell Orchards
    found += RevealIfNear(akSurveyor, 0x00096B6B) ; Camp McClintock
    found += RevealIfNear(akSurveyor, 0x00096B94) ; Relay Tower HN-B1-12
    found += RevealIfNear(akSurveyor, 0x00096FB0) ; Relay Tower DP-B5-21
    found += RevealIfNear(akSurveyor, 0x00097113) ; Relay Tower LW-B1-22
    found += RevealIfNear(akSurveyor, 0x000984A6) ; Relay Tower HG-B7-09
    found += RevealIfNear(akSurveyor, 0x000985FC) ; Relay Tower EM-B1-27
    found += RevealIfNear(akSurveyor, 0x0009A013) ; Ohio River Adventures
    found += RevealIfNear(akSurveyor, 0x0009A021) ; Ranger District Office
    found += RevealIfNear(akSurveyor, 0x0009A02B) ; Whitespring Lookout
    found += RevealIfNear(akSurveyor, 0x0009A033) ; Red Rocket Mega Stop
    found += RevealIfNear(akSurveyor, 0x0009A04A) ; The Giant Teapot
    found += RevealIfNear(akSurveyor, 0x0009A054) ; Pumpkin House
    found += RevealIfNear(akSurveyor, 0x0009A094) ; Landview Lighthouse
    found += RevealIfNear(akSurveyor, 0x0009A0B1) ; RobCo Research Center
    found += RevealIfNear(akSurveyor, 0x0009A0CA) ; Kanawha County Cemetery
    found += RevealIfNear(akSurveyor, 0x0009A0CE) ; Seneca Rocks
    found += RevealIfNear(akSurveyor, 0x0009A120) ; White Powder Winter Sports
    found += RevealIfNear(akSurveyor, 0x0009A15B) ; Sugar Grove
    found += RevealIfNear(akSurveyor, 0x0009A187) ; Summersville
    found += RevealIfNear(akSurveyor, 0x0009A214) ; Summersville Docks
    found += RevealIfNear(akSurveyor, 0x0009A248) ; New Gad
    found += RevealIfNear(akSurveyor, 0x0009A258) ; Torrance House
    found += RevealIfNear(akSurveyor, 0x0009A35A) ; Camp Venture
    found += RevealIfNear(akSurveyor, 0x0009A3EE) ; Sutton
    found += RevealIfNear(akSurveyor, 0x0009A452) ; The Whitespring Refuge
    found += RevealIfNear(akSurveyor, 0x0009A454) ; The Whitespring Bunker
    found += RevealIfNear(akSurveyor, 0x0009A45A) ; The Whitespring Golf Club
    Return found
EndFunction

Int Function RevealBatch02(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x0009A6C7) ; The Retreat
    found += RevealIfNear(akSurveyor, 0x000A1B86) ; North Kanawha Lookout
    found += RevealIfNear(akSurveyor, 0x000A7297) ; Gilman Lumber Mill
    found += RevealIfNear(akSurveyor, 0x000AF652) ; Vault 94
    found += RevealIfNear(akSurveyor, 0x000B1051) ; Vault 76
    found += RevealIfNear(akSurveyor, 0x000B1A65) ; Vault 96
    found += RevealIfNear(akSurveyor, 0x000B71BF) ; Wendigo Cave
    found += RevealIfNear(akSurveyor, 0x00109B9D) ; West Tek Research Center
    found += RevealIfNear(akSurveyor, 0x00109BAB) ; Wilson Brother's Auto Repair
    found += RevealIfNear(akSurveyor, 0x00109EA4) ; Converted Munitions Factory
    found += RevealIfNear(akSurveyor, 0x0010A073) ; Sunnytop Ski Lanes
    found += RevealIfNear(akSurveyor, 0x0010A076) ; Sons of Dane Compound
    found += RevealIfNear(akSurveyor, 0x0010A0EB) ; Grafton Dam
    found += RevealIfNear(akSurveyor, 0x0010A1CB) ; Crevasse Dam
    found += RevealIfNear(akSurveyor, 0x0010CC9C) ; Hornwright Industrial Headquarters
    found += RevealIfNear(akSurveyor, 0x0010CCA1) ; Charleston Trainyard
    found += RevealIfNear(akSurveyor, 0x0010CCE8) ; Charleston Landfill
    found += RevealIfNear(akSurveyor, 0x0010CCEC) ; Charleston
    found += RevealIfNear(akSurveyor, 0x0011293B) ; Arktos Pharma
    found += RevealIfNear(akSurveyor, 0x0012C668) ; Silva Homestead
    found += RevealIfNear(akSurveyor, 0x00131799) ; Overseer's Camp
    found += RevealIfNear(akSurveyor, 0x0013B87D) ; Abandoned Bunker
    found += RevealIfNear(akSurveyor, 0x0013E278) ; The Kill Box
    found += RevealIfNear(akSurveyor, 0x0013E296) ; Sugarmaple
    found += RevealIfNear(akSurveyor, 0x0013E299) ; Hornwright Summer Villa
    found += RevealIfNear(akSurveyor, 0x0017A042) ; Missile Silo Alpha
    found += RevealIfNear(akSurveyor, 0x0026B608) ; Dolly Sods Campground
    found += RevealIfNear(akSurveyor, 0x0029D611) ; New River Gorge Bridge - West
    found += RevealIfNear(akSurveyor, 0x0029D614) ; New River Gorge Bridge - East
    found += RevealIfNear(akSurveyor, 0x002B9652) ; Poseidon Energy Plant Yard
    found += RevealIfNear(akSurveyor, 0x002C5F13) ; The Whitespring
    found += RevealIfNear(akSurveyor, 0x002C7635) ; Foundation
    found += RevealIfNear(akSurveyor, 0x002C7638) ; Spruce Knob Campground
    found += RevealIfNear(akSurveyor, 0x002C8AEB) ; Sunnytop Station
    found += RevealIfNear(akSurveyor, 0x002C8CAF) ; Pleasant Valley Station
    found += RevealIfNear(akSurveyor, 0x002C8CB4) ; The Whitespring Station
    found += RevealIfNear(akSurveyor, 0x002C8E7D) ; R&G Station
    found += RevealIfNear(akSurveyor, 0x002C9329) ; Charleston Station
    found += RevealIfNear(akSurveyor, 0x002C94B1) ; Sutton Station
    found += RevealIfNear(akSurveyor, 0x002C963A) ; Morgantown Station
    found += RevealIfNear(akSurveyor, 0x002C97C5) ; Grafton Station
    found += RevealIfNear(akSurveyor, 0x002CCAD3) ; Thunder Mountain Substation TM-02
    found += RevealIfNear(akSurveyor, 0x002CD2E0) ; Monongah Power Substation MZ-01
    found += RevealIfNear(akSurveyor, 0x002CDB06) ; Monongah Power Substation MZ-02
    found += RevealIfNear(akSurveyor, 0x002CEB15) ; Poseidon Power Substation PX-02
    Return found
EndFunction

Int Function RevealBatch03(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x002CF31B) ; Poseidon Power Substation PX-01
    found += RevealIfNear(akSurveyor, 0x002CFB21) ; Poseidon Power Substation PX-03
    found += RevealIfNear(akSurveyor, 0x002DE583) ; Sunnytop Ski Lanes Base Lodge
    found += RevealIfNear(akSurveyor, 0x002DE837) ; Missile Silo Bravo
    found += RevealIfNear(akSurveyor, 0x002E5C43) ; Crashed Plane
    found += RevealIfNear(akSurveyor, 0x002F9B7E) ; Toxic Dried Lakebed
    found += RevealIfNear(akSurveyor, 0x0030927D) ; Braxson's Quality Medical Supplies
    found += RevealIfNear(akSurveyor, 0x00311411) ; Abbie's Bunker
    found += RevealIfNear(akSurveyor, 0x00311412) ; Ella Ames' Bunker
    found += RevealIfNear(akSurveyor, 0x00311413) ; Raleigh Clay's Bunker
    found += RevealIfNear(akSurveyor, 0x0032BC27) ; Monorail Elevator
    found += RevealIfNear(akSurveyor, 0x00356BEB) ; Horizon's Rest
    found += RevealIfNear(akSurveyor, 0x00356BED) ; Beckwith Farm
    found += RevealIfNear(akSurveyor, 0x00356BEF) ; South Cutthroat Camp
    found += RevealIfNear(akSurveyor, 0x00356BF2) ; North Cutthroat Camp
    found += RevealIfNear(akSurveyor, 0x00356BF4) ; Toxic Larry's Meat 'n Go
    found += RevealIfNear(akSurveyor, 0x00356BF6) ; Seneca Gang Camp
    found += RevealIfNear(akSurveyor, 0x00356BF8) ; Bailey Family Cabin
    found += RevealIfNear(akSurveyor, 0x00356BFA) ; Autumn Acre Cabin
    found += RevealIfNear(akSurveyor, 0x00356BFC) ; Skullbone Vantage
    found += RevealIfNear(akSurveyor, 0x00356BFE) ; The Rose Room
    found += RevealIfNear(akSurveyor, 0x00356C00) ; Mosstown
    found += RevealIfNear(akSurveyor, 0x00356C02) ; Big Maw
    found += RevealIfNear(akSurveyor, 0x00356C04) ; Kiddie Corner Cabins
    found += RevealIfNear(akSurveyor, 0x00356C06) ; Willard Corporate Housing
    found += RevealIfNear(akSurveyor, 0x00356C08) ; Mac's Farm
    found += RevealIfNear(akSurveyor, 0x00356C0A) ; Drop Site C2
    found += RevealIfNear(akSurveyor, 0x00356C0E) ; Quarry X3
    found += RevealIfNear(akSurveyor, 0x00356C10) ; Drop Site G3
    found += RevealIfNear(akSurveyor, 0x00356C16) ; Twin Pine Cabins
    found += RevealIfNear(akSurveyor, 0x00356C18) ; Camp Adams
    found += RevealIfNear(akSurveyor, 0x00356C28) ; The Sludge Hole
    found += RevealIfNear(akSurveyor, 0x00356C2C) ; The Vantage
    found += RevealIfNear(akSurveyor, 0x0037E6DE) ; Wixon Homestead
    found += RevealIfNear(akSurveyor, 0x00387995) ; East Mountain Lookout
    found += RevealIfNear(akSurveyor, 0x0038B442) ; Anchor Farm
    found += RevealIfNear(akSurveyor, 0x00391027) ; Pioneer Scout Lookout
    found += RevealIfNear(akSurveyor, 0x0039199A) ; Ripper Alley
    found += RevealIfNear(akSurveyor, 0x003919AC) ; Safe 'n Clean Disposal
    found += RevealIfNear(akSurveyor, 0x003919AF) ; Yellow Sandy's Still
    found += RevealIfNear(akSurveyor, 0x003919B1) ; Cliffwatch
    found += RevealIfNear(akSurveyor, 0x003919D5) ; Bleeding Kate's Grindhouse
    found += RevealIfNear(akSurveyor, 0x003919E6) ; Ammo Dump
    found += RevealIfNear(akSurveyor, 0x00391A02) ; Old Mold Quarry
    found += RevealIfNear(akSurveyor, 0x00391A08) ; Forward Station Alpha
    Return found
EndFunction

Int Function RevealBatch04(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x00391A0B) ; The Thorn
    found += RevealIfNear(akSurveyor, 0x00391A0E) ; Firebase Major
    found += RevealIfNear(akSurveyor, 0x00391A11) ; Firebase LT
    found += RevealIfNear(akSurveyor, 0x00391A18) ; Superior Sunset Farm
    found += RevealIfNear(akSurveyor, 0x00391A1D) ; Spruce Knob Lake
    found += RevealIfNear(akSurveyor, 0x00391A1F) ; Spruce Knob Channels
    found += RevealIfNear(akSurveyor, 0x00391A21) ; Lake Eloise
    found += RevealIfNear(akSurveyor, 0x00391A23) ; Twin Lakes
    found += RevealIfNear(akSurveyor, 0x00391A36) ; Slocum's Joe
    found += RevealIfNear(akSurveyor, 0x003942AF) ; Excelsior Model Home
    found += RevealIfNear(akSurveyor, 0x003942B2) ; The Freak Show
    found += RevealIfNear(akSurveyor, 0x003A07A1) ; South Mountain Lookout
    found += RevealIfNear(akSurveyor, 0x003A07A3) ; Flatwoods Lookout
    found += RevealIfNear(akSurveyor, 0x003A07A5) ; Camp Adams Lookout
    found += RevealIfNear(akSurveyor, 0x003A57D7) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A581A) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A587C) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A58DE) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A590D) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A5940) ; Fissure Site
    found += RevealIfNear(akSurveyor, 0x003A59B3) ; US-13C Bivouac
    found += RevealIfNear(akSurveyor, 0x003A5E73) ; East Ridge Lookout
    found += RevealIfNear(akSurveyor, 0x003A5E75) ; Ranger Lookout
    found += RevealIfNear(akSurveyor, 0x003A5E77) ; Dolly Sods Lookout
    found += RevealIfNear(akSurveyor, 0x003AFE29) ; Seneca Rocks Visitor Center
    found += RevealIfNear(akSurveyor, 0x003B2EFA) ; Smith Farm
    found += RevealIfNear(akSurveyor, 0x003B2F5B) ; Graninger Farm
    found += RevealIfNear(akSurveyor, 0x003B2F7E) ; Becker Farm
    found += RevealIfNear(akSurveyor, 0x003B771F) ; Dagger's Den
    found += RevealIfNear(akSurveyor, 0x003C69FA) ; Brim Quarry
    found += RevealIfNear(akSurveyor, 0x003D4B16) ; Thunder Mt. Power Plant Yard
    found += RevealIfNear(akSurveyor, 0x003F892E) ; Overseer's Home
    found += RevealIfNear(akSurveyor, 0x0040BD19) ; The Wayward
    found += RevealIfNear(akSurveyor, 0x0042B260) ; Gauley Mine
    found += RevealIfNear(akSurveyor, 0x00438591) ; Grafton Steel Yard
    found += RevealIfNear(akSurveyor, 0x004F55B2) ; Hunter's Ridge
    found += RevealIfNear(akSurveyor, 0x004F5970) ; Carson Family Bunker
    found += RevealIfNear(akSurveyor, 0x004F8A73) ; Veiled Sundew Grove
    found += RevealIfNear(akSurveyor, 0x004F8A74) ; Creekside Sundew Grove
    found += RevealIfNear(akSurveyor, 0x0050199D) ; Charleston Herald
    found += RevealIfNear(akSurveyor, 0x00501F9F) ; Sylvie & Sons Logging Camp
    found += RevealIfNear(akSurveyor, 0x00501FC1) ; Southhampton Estate
    found += RevealIfNear(akSurveyor, 0x0050934C) ; Pylon V-13
    found += RevealIfNear(akSurveyor, 0x0051C65A) ; Moonshiner's Shack
    found += RevealIfNear(akSurveyor, 0x0052534F) ; Highland Marsh
    Return found
EndFunction

Int Function RevealBatch05(Actor akSurveyor)
    Int found = 0
    found += RevealIfNear(akSurveyor, 0x005253AB) ; Gulper Lagoon
    found += RevealIfNear(akSurveyor, 0x005253D8) ; Gnarled Shallows
    found += RevealIfNear(akSurveyor, 0x005376F8) ; Crimson Prospect
    found += RevealIfNear(akSurveyor, 0x005570F5) ; The Deep
    found += RevealIfNear(akSurveyor, 0x00586D29) ; The Bounty
    found += RevealIfNear(akSurveyor, 0x00586D2B) ; The Pigsty
    found += RevealIfNear(akSurveyor, 0x00586D2F) ; Moth-Home
    found += RevealIfNear(akSurveyor, 0x00590275) ; Sacrament
    found += RevealIfNear(akSurveyor, 0x00597606) ; Carleton Mine
    found += RevealIfNear(akSurveyor, 0x0064D250) ; Foundation Outpost
    found += RevealIfNear(akSurveyor, 0x006EC023) ; Forward Station Tango
    found += RevealIfNear(akSurveyor, 0x006EE7CD) ; Foundation Outpost
    found += RevealIfNear(akSurveyor, 0x0072630D) ; Organ Cave South
    found += RevealIfNear(akSurveyor, 0x0072638E) ; Organ Cave South
    found += RevealIfNear(akSurveyor, 0x007263B8) ; Organ Cave South
    found += RevealIfNear(akSurveyor, 0x00869989) ; The Forest
    found += RevealIfNear(akSurveyor, 0x0086998C) ; Cranberry Bog
    found += RevealIfNear(akSurveyor, 0x00869990) ; Savage Divide
    Return found
EndFunction
