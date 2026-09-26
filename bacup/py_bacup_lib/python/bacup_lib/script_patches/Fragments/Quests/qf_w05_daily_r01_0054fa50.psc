; Wren's hand-off and turn-in ran through player-dialogue scenes whose fragments
; did not survive the FO76 strip. Activating Wren stands in for those conversations.
ReferenceAlias Function WrenAlias()
    Return GetAlias(1) as ReferenceAlias
EndFunction

Function EnsureWrenListener()
    ReferenceAlias wren = WrenAlias()
    If wren == None
        Return
    EndIf
    ObjectReference wrenRef = wren.GetReference()
    If wrenRef != None
        RegisterForRemoteEvent(wrenRef, "OnActivate")
    EndIf
EndFunction

Event OnQuestInit()
    EnsureWrenListener()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    EnsureWrenListener()
EndEvent

Event OnQuestShutdown()
    UnregisterForAllEvents()
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If !IsRunning() || Alias_DQR01Player == None
        Return
    EndIf
    Actor player = Alias_DQR01Player.GetActorReference()
    ReferenceAlias wren = WrenAlias()
    If player == None || akActionRef != player || wren == None || akSender != wren.GetReference()
        Return
    EndIf
    If IsStageDone(400)
        If !IsStageDone(410)
            SetStage(410)
        EndIf
    ElseIf !IsStageDone(200)
        SetStage(200)
    EndIf
EndEvent

; Debug and superseded ("OBSOLETE" in the stage notes) stages: bound by VMAD, but
; the shipped quest drives progression through 230-247 and DefaultAliasInventoryManagement.
Function Fragment_Stage_0000_Item_00()
EndFunction

Function Fragment_Stage_0001_Item_00()
EndFunction

Function Fragment_Stage_0103_Item_00()
EndFunction

Function Fragment_Stage_0104_Item_00()
EndFunction

Function Fragment_Stage_0105_Item_00()
EndFunction

Function Fragment_Stage_0210_Item_00()
EndFunction

Function Fragment_Stage_0211_Item_00()
EndFunction

Function Fragment_Stage_0212_Item_00()
EndFunction

Function Fragment_Stage_0213_Item_00()
EndFunction

Function Fragment_Stage_0214_Item_00()
EndFunction

Function Fragment_Stage_0215_Item_00()
EndFunction

Function Fragment_Stage_0216_Item_00()
EndFunction

Function Fragment_Stage_0217_Item_00()
EndFunction

Function Fragment_Stage_0250_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
EndFunction

Function Fragment_Stage_0220_Item_00()
EndFunction

Function Fragment_Stage_0221_Item_00()
EndFunction

Function Fragment_Stage_0222_Item_00()
EndFunction

Function Fragment_Stage_0223_Item_00()
EndFunction

Function Fragment_Stage_0224_Item_00()
EndFunction

Function Fragment_Stage_0225_Item_00()
EndFunction

Function Fragment_Stage_0226_Item_00()
EndFunction

Function Fragment_Stage_0227_Item_00()
EndFunction

; DefaultQuestRemovePlayersScript.RemoveQuestReference only takes items back out of
; the player's inventory, so a tube left lying in the world survives an abandoned or
; expired run and the next day places another one on top of it. Holding each placed
; tube in its TechItem alias lets this script delete the world copies at the run
; boundary, and it is also what arms the aliases' DefaultAliasOnContainerChangedTo
; pickup stages and objective 200's per-item targets.
Function PlaceTechItem(ReferenceAlias akPlacement, ReferenceAlias akItemAlias, MiscObject akItem)
    If akPlacement == None || akItemAlias == None || akItem == None
        Return
    EndIf
    ObjectReference marker = akPlacement.GetReference()
    If marker == None
        Return
    EndIf
    ObjectReference spawned = marker.PlaceAtMe(akItem)
    If spawned != None
        spawned.AddKeyword(W05_Daily_R01_TechKeyword)
        akItemAlias.ForceRefTo(spawned)
    EndIf
EndFunction

Function ClearPlacedTechItem(ReferenceAlias akItemAlias)
    If akItemAlias == None
        Return
    EndIf
    ObjectReference itemRef = akItemAlias.GetReference()
    If itemRef != None && itemRef.GetContainer() == None
        itemRef.Delete()
    EndIf
    akItemAlias.Clear()
EndFunction

Function ClearPlacedTechItems()
    ClearPlacedTechItem(Alias_TechItem01)
    ClearPlacedTechItem(Alias_TechItem02)
    ClearPlacedTechItem(Alias_TechItem03)
    ClearPlacedTechItem(Alias_TechItem04)
    ClearPlacedTechItem(Alias_TechItem05)
    ClearPlacedTechItem(Alias_TechItem06)
    ClearPlacedTechItem(Alias_TechItem07)
    ClearPlacedTechItem(Alias_TechItem08)
EndFunction

Function Fragment_Stage_0230_Item_00()
    PlaceTechItem(Alias_TechItemPlacement01, Alias_TechItem01, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0231_Item_00()
    PlaceTechItem(Alias_TechItemPlacement02, Alias_TechItem02, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0232_Item_00()
    PlaceTechItem(Alias_TechItemPlacement03, Alias_TechItem03, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0233_Item_00()
    PlaceTechItem(Alias_TechItemPlacement04, Alias_TechItem04, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0234_Item_00()
    PlaceTechItem(Alias_TechItemPlacement05, Alias_TechItem05, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0235_Item_00()
    PlaceTechItem(Alias_TechItemPlacement06, Alias_TechItem06, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0236_Item_00()
    PlaceTechItem(Alias_TechItemPlacement07, Alias_TechItem07, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0237_Item_00()
    PlaceTechItem(Alias_TechItemPlacement08, Alias_TechItem08, W05_Daily_R01_Tech)
EndFunction

Function Fragment_Stage_0240_Item_00()
    PlaceTechItem(Alias_TechItemPlacement01, Alias_TechItem01, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0241_Item_00()
    PlaceTechItem(Alias_TechItemPlacement02, Alias_TechItem02, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0242_Item_00()
    PlaceTechItem(Alias_TechItemPlacement03, Alias_TechItem03, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0243_Item_00()
    PlaceTechItem(Alias_TechItemPlacement04, Alias_TechItem04, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0244_Item_00()
    PlaceTechItem(Alias_TechItemPlacement05, Alias_TechItem05, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0245_Item_00()
    PlaceTechItem(Alias_TechItemPlacement06, Alias_TechItem06, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0246_Item_00()
    PlaceTechItem(Alias_TechItemPlacement07, Alias_TechItem07, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0247_Item_00()
    PlaceTechItem(Alias_TechItemPlacement08, Alias_TechItem08, W05_Daily_R01_TechBroken)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(400, True)
    Actor player = None
    If Alias_DQR01Player != None
        player = Alias_DQR01Player.GetActorReference()
    EndIf
    If player != None
        If W05_Daily_R01_Tech != None
            player.RemoveItem(W05_Daily_R01_Tech, player.GetItemCount(W05_Daily_R01_Tech), True)
        EndIf
        If W05_Daily_R01_TechBroken != None
            player.RemoveItem(W05_Daily_R01_TechBroken, player.GetItemCount(W05_Daily_R01_TechBroken), True)
        EndIf
    EndIf
    SetStage(9000)
EndFunction

; Caps, scrip and Treasury Notes for 9000 come from the converter-attached
; B21:QuestRewards / B21:CurrencyQuestRewards rows; only reputation is local.
Function Fragment_Stage_9000_Item_00()
    Actor player = None
    If Alias_DQR01Player != None
        player = Alias_DQR01Player.GetActorReference()
    EndIf
    If player != None && Reputation_AV_Crater != None && Rep_Mod_DailyR_Add != None
        player.ModValue(Reputation_AV_Crater, Rep_Mod_DailyR_Add.GetValue())
    EndIf
    SetObjectiveCompleted(400, True)
    SetStage(10000)
EndFunction

Function Fragment_Stage_9990_Item_00()
    ClearPlacedTechItems()
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveDisplayed(400, False)
    UnregisterForAllEvents()
    Stop()
EndFunction

Function Fragment_Stage_10000_Item_00()
    ClearPlacedTechItems()
    UnregisterForAllEvents()
    Stop()
EndFunction
