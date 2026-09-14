Event OnAliasInit()
    myPlayer = GetReference() as Actor
    myQuest = GetOwningQuest()
    If MQ_Overseer_HolotapesList != None
        AddInventoryEventFilter(MQ_Overseer_HolotapesList)
    EndIf

    PrepareExistingQuestObject(MQ_Overseer_01_Vault76Holotape, QuestObject01)
    PrepareExistingQuestObject(MQ_Overseer_01A_CAMPHolotape, QuestObject01A)
    PrepareExistingQuestObject(MQ_Overseer_02_FlatwoodsHolotape, QuestObject02)
    PrepareExistingQuestObject(MQ_Overseer_03_MorgantownHQHolotape, QuestObject03)
    PrepareExistingQuestObject(MQ_Overseer_04_FirehouseHolotape, QuestObject04)
    PrepareExistingQuestObject(MQ_Overseer_05_TopOfTheWorldHolotape, QuestObject05)
    PrepareExistingQuestObject(MQ_Overseer_06_FreeStatesHolotape, QuestObject06)
    PrepareExistingQuestObject(MQ_Overseer_07_CharlestonHolotape, QuestObject07)
    PrepareExistingQuestObject(MQ_Overseer_08_CampVentureHolotape, QuestObject08)
    PrepareExistingQuestObject(MQ_Overseer_09_AlleghenyHolotape, QuestObject09)
    PrepareExistingQuestObject(MQ_Overseer_10_CampMcClintockHolotape, QuestObject10)
    PrepareExistingQuestObject(MQ_Overseer_11_BackAtDefianceHolotape, QuestObject11)
    PrepareExistingQuestObject(MQ_Overseer_12_NukesHolotape, QuestObject12)
    PrepareExistingQuestObject(MQ_Overseer_X1_NukeSiloHolotape, QuestObjectX1)
    PrepareExistingQuestObject(MQ_Overseer_X2_GraftonHolotape, QuestObjectX2)
    PrepareExistingQuestObject(MQ_Overseer_X3_MountainHolotape, QuestObjectX3)
    PrepareExistingQuestObject(MQ_Overseer_X4_NukeSiloHolotape, QuestObjectX4)
    PrepareExistingQuestObject(MQ_Overseer_X5_NukeSiloHolotape, QuestObjectX5)

    ReconcileHolotapePlayRegistrations()
    SyncHolotapesAlreadyHeld()
EndEvent

Event OnPlayerLoadGame()
    myPlayer = GetReference() as Actor
    myQuest = GetOwningQuest()
    If MQ_Overseer_HolotapesList != None
        AddInventoryEventFilter(MQ_Overseer_HolotapesList)
    EndIf
    ReconcileHolotapePlayRegistrations()
    SyncHolotapesAlreadyHeld()
EndEvent

Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem == MQ_Overseer_01_Vault76Holotape
        RegisterHolotape(MQ_Overseer_01_Vault76Holotape, akItemReference, QuestObject01, 10)
    ElseIf akBaseItem == MQ_Overseer_01A_CAMPHolotape
        RegisterHolotape(MQ_Overseer_01A_CAMPHolotape, akItemReference, QuestObject01A, 16)
    ElseIf akBaseItem == MQ_Overseer_02_FlatwoodsHolotape
        RegisterHolotape(MQ_Overseer_02_FlatwoodsHolotape, akItemReference, QuestObject02, 22)
    ElseIf akBaseItem == MQ_Overseer_03_MorgantownHQHolotape
        RegisterHolotape(MQ_Overseer_03_MorgantownHQHolotape, akItemReference, QuestObject03, 32)
    ElseIf akBaseItem == MQ_Overseer_04_FirehouseHolotape
        RegisterHolotape(MQ_Overseer_04_FirehouseHolotape, akItemReference, QuestObject04, 42)
    ElseIf akBaseItem == MQ_Overseer_05_TopOfTheWorldHolotape
        RegisterHolotape(MQ_Overseer_05_TopOfTheWorldHolotape, akItemReference, QuestObject05, 52)
    ElseIf akBaseItem == MQ_Overseer_06_FreeStatesHolotape
        RegisterHolotape(MQ_Overseer_06_FreeStatesHolotape, akItemReference, QuestObject06, 62)
    ElseIf akBaseItem == MQ_Overseer_07_CharlestonHolotape
        RegisterHolotape(MQ_Overseer_07_CharlestonHolotape, akItemReference, QuestObject07, 72)
    ElseIf akBaseItem == MQ_Overseer_08_CampVentureHolotape
        RegisterHolotape(MQ_Overseer_08_CampVentureHolotape, akItemReference, QuestObject08, 82)
    ElseIf akBaseItem == MQ_Overseer_09_AlleghenyHolotape
        RegisterHolotape(MQ_Overseer_09_AlleghenyHolotape, akItemReference, QuestObject09, 92)
    ElseIf akBaseItem == MQ_Overseer_10_CampMcClintockHolotape
        RegisterHolotape(MQ_Overseer_10_CampMcClintockHolotape, akItemReference, QuestObject10, 102)
    ElseIf akBaseItem == MQ_Overseer_11_BackAtDefianceHolotape
        RegisterHolotape(MQ_Overseer_11_BackAtDefianceHolotape, akItemReference, QuestObject11, 112)
    ElseIf akBaseItem == MQ_Overseer_12_NukesHolotape
        RegisterHolotape(MQ_Overseer_12_NukesHolotape, akItemReference, QuestObject12, 122)
    ElseIf akBaseItem == MQ_Overseer_X1_NukeSiloHolotape
        RegisterHolotape(MQ_Overseer_X1_NukeSiloHolotape, akItemReference, QuestObjectX1, 502)
    ElseIf akBaseItem == MQ_Overseer_X2_GraftonHolotape
        RegisterHolotape(MQ_Overseer_X2_GraftonHolotape, akItemReference, QuestObjectX2, 512)
    ElseIf akBaseItem == MQ_Overseer_X3_MountainHolotape
        RegisterHolotape(MQ_Overseer_X3_MountainHolotape, akItemReference, QuestObjectX3, 522)
    ElseIf akBaseItem == MQ_Overseer_X4_NukeSiloHolotape
        RegisterHolotape(MQ_Overseer_X4_NukeSiloHolotape, akItemReference, QuestObjectX4, 532)
    ElseIf akBaseItem == MQ_Overseer_X5_NukeSiloHolotape
        RegisterHolotape(MQ_Overseer_X5_NukeSiloHolotape, akItemReference, QuestObjectX5, 542)
    EndIf
EndEvent

; Keep equip as a fallback for inventories restored from older converted saves.
Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    SetHolotapeStage(PlayedStageFor(akBaseObject))
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    If akSender != None
        SetHolotapeStage(PlayedStageFor(akSender.GetBaseObject()))
    EndIf
EndEvent

Event OnAliasShutdown()
    ClearHolotapePlayRegistrations()
    RemoveAllInventoryEventFilters()
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If akNewLoc == None
        Return
    EndIf

    SetStageOnLocation(akNewLoc, LocForestOverseersCAMPLocation, 18)
    SetStageOnLocation(akNewLoc, LocForestFlatwoodsTavernLocation, 20)
    SetStageOnLocation(akNewLoc, MorgantownAirfieldLocation, 30)
    SetStageOnLocation(akNewLoc, CharlestonFireDeptLocation, 40)
    SetStageOnLocation(akNewLoc, TopOfTheWorldLocation, 50)
    SetStageOnLocation(akNewLoc, SubSwampAbbiesBunkerLocation, 60)
    SetStageOnLocation(akNewLoc, CharlestonCapitolLocation, 70)
    SetStageOnLocation(akNewLoc, SurvivalistTrainingCenterLocation, 80)
    ; Allegheny Asylum and Fort Defiance are one building holding two of the logs.
    SetStageOnLocation(akNewLoc, AlleghenyAsylumInteriorLocation, 90)
    SetStageOnLocation(akNewLoc, AlleghenyAsylumInteriorLocation, 110)
    SetStageOnLocation(akNewLoc, LocForestCampMcClintockLocation, 100)
    SetStageOnLocation(akNewLoc, MonongahMissileSiloLocation, 500)
    SetStageOnLocation(akNewLoc, LocToxicGraftonLocation, 510)
    SetStageOnLocation(akNewLoc, MountainsideBBLocation, 520)
    SetStageOnLocation(akNewLoc, SugarGroveMissileSiloExteriorLocation, 530)
    SetStageOnLocation(akNewLoc, SpruceKnobMissileSiloExteriorLocation, 540)
EndEvent

Function RegisterHolotape(Holotape holotapeBase, ObjectReference itemReference, ReferenceAlias targetAlias, Int pickedUpStage)
    If itemReference != None
        FillQuestObjectAlias(targetAlias, itemReference)
    Else
        PrepareExistingQuestObject(holotapeBase, targetAlias)
    EndIf
    SetHolotapeStage(pickedUpStage)
EndFunction

Function SetHolotapeStage(Int aiStage)
    If aiStage <= 0
        Return
    EndIf
    If myQuest == None
        myQuest = GetOwningQuest()
    EndIf
    If myQuest != None && !myQuest.IsStageDone(aiStage)
        myQuest.SetStage(aiStage)
    EndIf
EndFunction

Function SetStageOnLocation(Location akNewLoc, Location akTargetLoc, Int aiStage)
    ; Location.IsChild is parent-first: akTargetLoc.IsChild(akNewLoc) asks whether
    ; the location just entered sits inside the log's location.
    If akTargetLoc != None && (akNewLoc == akTargetLoc || akTargetLoc.IsChild(akNewLoc))
        SetHolotapeStage(aiStage)
    EndIf
EndFunction

Int Function PlayedStageFor(Form akBaseObject)
    If akBaseObject == MQ_Overseer_01_Vault76Holotape
        Return 15
    ElseIf akBaseObject == MQ_Overseer_01A_CAMPHolotape
        Return 17
    ElseIf akBaseObject == MQ_Overseer_02_FlatwoodsHolotape
        Return 25
    ElseIf akBaseObject == MQ_Overseer_03_MorgantownHQHolotape
        Return 35
    ElseIf akBaseObject == MQ_Overseer_04_FirehouseHolotape
        Return 45
    ElseIf akBaseObject == MQ_Overseer_05_TopOfTheWorldHolotape
        Return 55
    ElseIf akBaseObject == MQ_Overseer_06_FreeStatesHolotape
        Return 65
    ElseIf akBaseObject == MQ_Overseer_07_CharlestonHolotape
        Return 75
    ElseIf akBaseObject == MQ_Overseer_08_CampVentureHolotape
        Return 85
    ElseIf akBaseObject == MQ_Overseer_09_AlleghenyHolotape
        Return 95
    ElseIf akBaseObject == MQ_Overseer_10_CampMcClintockHolotape
        Return 105
    ElseIf akBaseObject == MQ_Overseer_11_BackAtDefianceHolotape
        Return 115
    ElseIf akBaseObject == MQ_Overseer_12_NukesHolotape
        Return 125
    ElseIf akBaseObject == MQ_Overseer_X1_NukeSiloHolotape
        Return 505
    ElseIf akBaseObject == MQ_Overseer_X2_GraftonHolotape
        Return 515
    ElseIf akBaseObject == MQ_Overseer_X3_MountainHolotape
        Return 525
    ElseIf akBaseObject == MQ_Overseer_X4_NukeSiloHolotape
        Return 535
    ElseIf akBaseObject == MQ_Overseer_X5_NukeSiloHolotape
        Return 545
    EndIf
    Return 0
EndFunction

; A log can already be in the player's inventory before the quest starts, so
; replay the pick-up stages for everything already held.
Function SyncHolotapesAlreadyHeld()
    If myPlayer == None
        Return
    EndIf

    SyncHolotapeAlreadyHeld(MQ_Overseer_01_Vault76Holotape, 10)
    SyncHolotapeAlreadyHeld(MQ_Overseer_01A_CAMPHolotape, 16)
    SyncHolotapeAlreadyHeld(MQ_Overseer_02_FlatwoodsHolotape, 22)
    SyncHolotapeAlreadyHeld(MQ_Overseer_03_MorgantownHQHolotape, 32)
    SyncHolotapeAlreadyHeld(MQ_Overseer_04_FirehouseHolotape, 42)
    SyncHolotapeAlreadyHeld(MQ_Overseer_05_TopOfTheWorldHolotape, 52)
    SyncHolotapeAlreadyHeld(MQ_Overseer_06_FreeStatesHolotape, 62)
    SyncHolotapeAlreadyHeld(MQ_Overseer_07_CharlestonHolotape, 72)
    SyncHolotapeAlreadyHeld(MQ_Overseer_08_CampVentureHolotape, 82)
    SyncHolotapeAlreadyHeld(MQ_Overseer_09_AlleghenyHolotape, 92)
    SyncHolotapeAlreadyHeld(MQ_Overseer_10_CampMcClintockHolotape, 102)
    SyncHolotapeAlreadyHeld(MQ_Overseer_11_BackAtDefianceHolotape, 112)
    SyncHolotapeAlreadyHeld(MQ_Overseer_12_NukesHolotape, 122)
    SyncHolotapeAlreadyHeld(MQ_Overseer_X1_NukeSiloHolotape, 502)
    SyncHolotapeAlreadyHeld(MQ_Overseer_X2_GraftonHolotape, 512)
    SyncHolotapeAlreadyHeld(MQ_Overseer_X3_MountainHolotape, 522)
    SyncHolotapeAlreadyHeld(MQ_Overseer_X4_NukeSiloHolotape, 532)
    SyncHolotapeAlreadyHeld(MQ_Overseer_X5_NukeSiloHolotape, 542)
EndFunction

Function SyncHolotapeAlreadyHeld(Holotape holotapeBase, Int pickedUpStage)
    If holotapeBase != None && myPlayer.GetItemCount(holotapeBase) > 0
        SetHolotapeStage(pickedUpStage)
    EndIf
EndFunction

Function PrepareExistingQuestObject(Holotape holotapeBase, ReferenceAlias targetAlias)
    If myPlayer == None || holotapeBase == None || targetAlias == None
        Return
    EndIf
    If myPlayer.GetItemCount(holotapeBase) == 0 || targetAlias.GetReference() != None
        Return
    EndIf

    ObjectReference itemReference = myPlayer.PlaceAtMe(holotapeBase, 1, false, true, false)
    If itemReference == None
        Return
    EndIf
    itemReference.Enable()

    myPlayer.RemoveItem(holotapeBase, 1, true)
    targetAlias.ForceRefTo(itemReference)
    myPlayer.AddItem(itemReference, 1, true)
    RegisterHolotapePlay(targetAlias)
EndFunction

Function FillQuestObjectAlias(ReferenceAlias targetAlias, ObjectReference itemReference)
    If targetAlias != None && targetAlias.GetReference() != itemReference
        ObjectReference previousReference = targetAlias.GetReference()
        If previousReference != None
            UnregisterForRemoteEvent(previousReference, "OnHolotapePlay")
        EndIf
        targetAlias.ForceRefTo(itemReference)
        RegisterHolotapePlay(targetAlias)
    EndIf
EndFunction

Function ReconcileHolotapePlayRegistrations()
    ClearHolotapePlayRegistrations()
    RegisterHolotapePlay(QuestObject01)
    RegisterHolotapePlay(QuestObject01A)
    RegisterHolotapePlay(QuestObject02)
    RegisterHolotapePlay(QuestObject03)
    RegisterHolotapePlay(QuestObject04)
    RegisterHolotapePlay(QuestObject05)
    RegisterHolotapePlay(QuestObject06)
    RegisterHolotapePlay(QuestObject07)
    RegisterHolotapePlay(QuestObject08)
    RegisterHolotapePlay(QuestObject09)
    RegisterHolotapePlay(QuestObject10)
    RegisterHolotapePlay(QuestObject11)
    RegisterHolotapePlay(QuestObject12)
    RegisterHolotapePlay(QuestObjectX1)
    RegisterHolotapePlay(QuestObjectX2)
    RegisterHolotapePlay(QuestObjectX3)
    RegisterHolotapePlay(QuestObjectX4)
    RegisterHolotapePlay(QuestObjectX5)
EndFunction

Function ClearHolotapePlayRegistrations()
    UnregisterHolotapePlay(QuestObject01)
    UnregisterHolotapePlay(QuestObject01A)
    UnregisterHolotapePlay(QuestObject02)
    UnregisterHolotapePlay(QuestObject03)
    UnregisterHolotapePlay(QuestObject04)
    UnregisterHolotapePlay(QuestObject05)
    UnregisterHolotapePlay(QuestObject06)
    UnregisterHolotapePlay(QuestObject07)
    UnregisterHolotapePlay(QuestObject08)
    UnregisterHolotapePlay(QuestObject09)
    UnregisterHolotapePlay(QuestObject10)
    UnregisterHolotapePlay(QuestObject11)
    UnregisterHolotapePlay(QuestObject12)
    UnregisterHolotapePlay(QuestObjectX1)
    UnregisterHolotapePlay(QuestObjectX2)
    UnregisterHolotapePlay(QuestObjectX3)
    UnregisterHolotapePlay(QuestObjectX4)
    UnregisterHolotapePlay(QuestObjectX5)
EndFunction

Function RegisterHolotapePlay(ReferenceAlias targetAlias)
    ObjectReference targetReference = None
    If targetAlias != None
        targetReference = targetAlias.GetReference()
    EndIf
    If targetReference != None
        RegisterForRemoteEvent(targetReference, "OnHolotapePlay")
    EndIf
EndFunction

Function UnregisterHolotapePlay(ReferenceAlias targetAlias)
    ObjectReference targetReference = None
    If targetAlias != None
        targetReference = targetAlias.GetReference()
    EndIf
    If targetReference != None
        UnregisterForRemoteEvent(targetReference, "OnHolotapePlay")
    EndIf
EndFunction
