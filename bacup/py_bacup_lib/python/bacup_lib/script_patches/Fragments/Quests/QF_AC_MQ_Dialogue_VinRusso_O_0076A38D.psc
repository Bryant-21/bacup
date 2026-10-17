; FO76 started the AC_MQ_Dialogue_VinRusso quest module here. FO4 starts the
; module quest (7456A7) directly; conversion makes its Loc and Actor_Vin aliases
; Optional because Loc was an event-data fill, and FO4 cannot refill Actor_Vin
; from Loc, so this parent forces its own location and the Vin reference its
; LCTN lists under AC_MQ_VinRusso_OffQuest_LCRT. Both off-quest parents run this
; at game start; an enabled Vin replaces a disabled one.
Function Fragment_Stage_0100_Item_00()
    Quest vinDialogue = Game.GetFormFromFile(0x007456A7, "SeventySix.esm") as Quest
    ObjectReference vinRef = Game.GetFormFromFile(0x0076A39B, "SeventySix.esm") as ObjectReference
    If vinDialogue == None || vinRef == None
        Return
    EndIf
    If !vinDialogue.IsRunning()
        vinDialogue.Start()
    EndIf
    LocationAlias moduleLoc = vinDialogue.GetAlias(1) as LocationAlias
    ReferenceAlias moduleVin = vinDialogue.GetAlias(2) as ReferenceAlias
    If moduleLoc == None || moduleVin == None
        Return
    EndIf
    ObjectReference currentVin = moduleVin.GetReference()
    If currentVin == None || (currentVin.IsDisabled() && !vinRef.IsDisabled())
        moduleLoc.ForceLocationTo(Alias_Loc.GetLocation())
        moduleVin.ForceRefTo(vinRef)
    EndIf
EndFunction
