Scriptname B21:EnclaveEventSupport Hidden
{Shared helpers for the converted Enclave events (Bots on Parade, Dropped
Connection, A Real Blast). FO76 requested regional encounter waves from server
services; these helpers materialize actors from the source WAVE records that
the site's location encounter properties select.}

; Species codes follow the source ESSChanceMain actor values:
; 0 super mutants, 1 scorched, 2 mole miners, 3 feral ghouls, 4 vicious dogs,
; 5 molerats, 6 rad rats, 7 liberators, 8 robots, 9 bloatflies, 10 cave crickets.
Form Function SpeciesForm(Int aiSpecies, Bool abBoss) Global
    Int formID = 0
    If aiSpecies == 0
        If abBoss
            formID = PickOne(0x00075341, 0x00117D85)
        Else
            formID = PickOne(0x00075335, 0x00088F14, 0x000948B3)
        EndIf
    ElseIf aiSpecies == 1
        formID = PickOne(0x0008E624, 0x0031B1FF)
    ElseIf aiSpecies == 2
        If abBoss
            formID = PickOne(0x0008EC17, 0x0008EC18)
        Else
            formID = PickOne(0x0008EC17, 0x0008EC18, 0x000342C9, 0x000FA89F)
        EndIf
    ElseIf aiSpecies == 3
        If abBoss
            formID = 0x00075343
        Else
            formID = 0x00075337
        EndIf
    ElseIf aiSpecies == 4
        If abBoss
            formID = 0x003CD324
        Else
            formID = 0x003CD322
        EndIf
    ElseIf aiSpecies == 5
        If abBoss
            formID = 0x001FDE69
        Else
            formID = 0x000342C9
        EndIf
    ElseIf aiSpecies == 6
        formID = 0x00112EFE
    ElseIf aiSpecies == 7
        If abBoss
            formID = 0x005A77D2
        Else
            formID = 0x00002ECE
        EndIf
    ElseIf aiSpecies == 8
        formID = PickOne(0x00450123, 0x00450122)
    ElseIf aiSpecies == 9
        formID = 0x0003320A
    Else
        formID = 0x00112A1C
    EndIf
    Return Game.GetFormFromFile(formID, "SeventySix.esm")
EndFunction

Int Function PickOne(Int aiFirst, Int aiSecond = 0, Int aiThird = 0, Int aiFourth = 0) Global
    Int count = 1
    If aiSecond != 0
        count += 1
    EndIf
    If aiThird != 0
        count += 1
    EndIf
    If aiFourth != 0
        count += 1
    EndIf
    Int pick = Utility.RandomInt(0, count - 1)
    If pick == 1
        Return aiSecond
    ElseIf pick == 2
        Return aiThird
    ElseIf pick == 3
        Return aiFourth
    EndIf
    Return aiFirst
EndFunction

Bool Function CenterInLocation(ObjectReference akCenter, Int aiLocationID) Global
    Location site = Game.GetFormFromFile(aiLocationID, "SeventySix.esm") as Location
    Return akCenter != None && site != None && akCenter.IsInLocation(site)
EndFunction

; Source spawn centers are SpawnCenter_Min1280_Max4096 style markers: actors
; appear disabled on a ring around the marker and walk in once enabled.
Actor Function PlaceOnRing(ObjectReference akCenter, Form akBase, Float afMinRadius = 1280.0, Float afMaxRadius = 2560.0) Global
    If akCenter == None || akBase == None
        Return None
    EndIf
    Actor spawned = akCenter.PlaceAtMe(akBase, 1, False, True, False) as Actor
    If spawned == None
        Return None
    EndIf
    Float angle = Utility.RandomFloat(0.0, 360.0)
    Float radius = Utility.RandomFloat(afMinRadius, afMaxRadius)
    spawned.MoveTo(akCenter, radius * Math.sin(angle), radius * Math.cos(angle), 0.0, False)
    spawned.MoveToNearestNavmeshLocation()
    Return spawned
EndFunction

; Corpses stay lootable. Living enemies the player can still see stay in the
; world; unloaded ones leave with the event.
Function ReleaseUnloadedActors(Actor[] akActors) Global
    Int index = 0
    While akActors != None && index < akActors.Length
        Actor spawned = akActors[index]
        If spawned != None && !spawned.IsDead() && !spawned.Is3DLoaded()
            spawned.DisableNoWait()
            spawned.Delete()
        EndIf
        index += 1
    EndWhile
EndFunction
