Event OnAliasInit()
    InitializePlayerAlias()
    ReconcilePlayerState()
EndEvent

Event OnPlayerLoadGame()
    InitializePlayerAlias()
    ReconcilePlayerState()
EndEvent

Function InitializePlayerAlias()
    myPlayer = GetReference() as Actor
    myQuest = GetOwningQuest()
    If OverseerPersonal_HolotapesList != None
        AddInventoryEventFilter(OverseerPersonal_HolotapesList)
    EndIf
    If OverseerPersonalMineKeycard != None
        AddInventoryEventFilter(OverseerPersonalMineKeycard)
    EndIf
EndFunction

Function ReconcilePlayerState()
    If myPlayer == None || myQuest == None
        Return
    EndIf
    PrepareExistingQuestObject(OverseerPersonal_01_VTecAgHolotape, QuestObject01_VTecAg)
    PrepareExistingQuestObject(OverseerPersonal_02_FamilyHolotape, QuestObject02_Family)
    PrepareExistingQuestObject(OverseerPersonal_03_HighSchoolHolotape, QuestObject03_HighSchool)
    PrepareExistingQuestObject(OverseerPersonal_04_UniversityHolotape, QuestObject04_University)
    PrepareExistingQuestObject(OverseerPersonal_05_HouseHolotape, QuestObject05_House)
    PrepareExistingQuestObject(OverseerPersonal_06_MineHolotape, QuestObject06_Mine)

    SetStageForOwnedForm(OverseerPersonal_01_VTecAgHolotape, 10)
    SetStageForOwnedForm(OverseerPersonal_02_FamilyHolotape, 20)
    SetStageForOwnedForm(OverseerPersonal_03_HighSchoolHolotape, 30)
    SetStageForOwnedForm(OverseerPersonal_04_UniversityHolotape, 40)
    SetStageForOwnedForm(OverseerPersonal_05_HouseHolotape, 50)
    SetStageForOwnedForm(OverseerPersonal_06_MineHolotape, 60)
    SetStageForOwnedForm(OverseerPersonalMineKeycard, 67)

    Location currentLocation = myPlayer.GetCurrentLocation()
    If currentLocation != None && SubMTRMountBlairWarehouseBasementLocation != None && (currentLocation == SubMTRMountBlairWarehouseBasementLocation || SubMTRMountBlairWarehouseBasementLocation.IsChild(currentLocation)) && !myQuest.IsStageDone(70)
        myQuest.SetStage(70)
    EndIf
    ReconcileHolotapePlayRegistrations()
EndFunction

Event OnItemAdded(Form akBaseItem, int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If myQuest == None || aiItemCount <= 0
        Return
    EndIf

    If akBaseItem == OverseerPersonal_01_VTecAgHolotape
        FillQuestObjectAlias(QuestObject01_VTecAg, akItemReference)
        SetStageOnce(10)
    ElseIf akBaseItem == OverseerPersonal_02_FamilyHolotape
        FillQuestObjectAlias(QuestObject02_Family, akItemReference)
        SetStageOnce(20)
    ElseIf akBaseItem == OverseerPersonal_03_HighSchoolHolotape
        FillQuestObjectAlias(QuestObject03_HighSchool, akItemReference)
        SetStageOnce(30)
    ElseIf akBaseItem == OverseerPersonal_04_UniversityHolotape
        FillQuestObjectAlias(QuestObject04_University, akItemReference)
        SetStageOnce(40)
    ElseIf akBaseItem == OverseerPersonal_05_HouseHolotape
        FillQuestObjectAlias(QuestObject05_House, akItemReference)
        SetStageOnce(50)
    ElseIf akBaseItem == OverseerPersonal_06_MineHolotape
        FillQuestObjectAlias(QuestObject06_Mine, akItemReference)
        SetStageOnce(60)
    ElseIf akBaseItem == OverseerPersonalMineKeycard
        SetStageOnce(67)
    EndIf
EndEvent

Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
    If akBaseObject == OverseerPersonal_01_VTecAgHolotape
        SetStageOnce(15)
    ElseIf akBaseObject == OverseerPersonal_02_FamilyHolotape
        SetStageOnce(25)
    ElseIf akBaseObject == OverseerPersonal_03_HighSchoolHolotape
        SetStageOnce(35)
    ElseIf akBaseObject == OverseerPersonal_04_UniversityHolotape
        SetStageOnce(45)
    ElseIf akBaseObject == OverseerPersonal_05_HouseHolotape
        SetStageOnce(55)
    ElseIf akBaseObject == OverseerPersonal_06_MineHolotape
        SetStageOnce(65)
    EndIf
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    If akSender == None
        Return
    EndIf

    Form playedHolotape = akSender.GetBaseObject()
    If playedHolotape == OverseerPersonal_01_VTecAgHolotape
        SetStageOnce(15)
    ElseIf playedHolotape == OverseerPersonal_02_FamilyHolotape
        SetStageOnce(25)
    ElseIf playedHolotape == OverseerPersonal_03_HighSchoolHolotape
        SetStageOnce(35)
    ElseIf playedHolotape == OverseerPersonal_04_UniversityHolotape
        SetStageOnce(45)
    ElseIf playedHolotape == OverseerPersonal_05_HouseHolotape
        SetStageOnce(55)
    ElseIf playedHolotape == OverseerPersonal_06_MineHolotape
        SetStageOnce(65)
    EndIf
EndEvent

Event OnAliasShutdown()
    ClearHolotapePlayRegistrations()
    RemoveAllInventoryEventFilters()
EndEvent

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If myQuest != None && akNewLoc != None && SubMTRMountBlairWarehouseBasementLocation != None && (akNewLoc == SubMTRMountBlairWarehouseBasementLocation || SubMTRMountBlairWarehouseBasementLocation.IsChild(akNewLoc))
        SetStageOnce(70)
    EndIf
EndEvent

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
    If targetAlias != None && itemReference != None && targetAlias.GetReference() == None
        targetAlias.ForceRefTo(itemReference)
        RegisterHolotapePlay(targetAlias)
    EndIf
EndFunction

Function ReconcileHolotapePlayRegistrations()
    ClearHolotapePlayRegistrations()
    RegisterHolotapePlay(QuestObject01_VTecAg)
    RegisterHolotapePlay(QuestObject02_Family)
    RegisterHolotapePlay(QuestObject03_HighSchool)
    RegisterHolotapePlay(QuestObject04_University)
    RegisterHolotapePlay(QuestObject05_House)
    RegisterHolotapePlay(QuestObject06_Mine)
EndFunction

Function ClearHolotapePlayRegistrations()
    UnregisterHolotapePlay(QuestObject01_VTecAg)
    UnregisterHolotapePlay(QuestObject02_Family)
    UnregisterHolotapePlay(QuestObject03_HighSchool)
    UnregisterHolotapePlay(QuestObject04_University)
    UnregisterHolotapePlay(QuestObject05_House)
    UnregisterHolotapePlay(QuestObject06_Mine)
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

Function SetStageForOwnedForm(Form akBaseItem, Int aiStage)
    If akBaseItem != None && myPlayer.GetItemCount(akBaseItem) > 0
        SetStageOnce(aiStage)
    EndIf
EndFunction

Function SetStageOnce(Int aiStage)
    If myQuest != None && !myQuest.IsStageDone(aiStage)
        myQuest.SetStage(aiStage)
    EndIf
EndFunction
