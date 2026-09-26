Event OnLocationChange(Location akOldLoc, Location akNewLoc)
    If akNewLoc == None
        Return
    EndIf

    ObjectReference player = Self.GetReference()
    If player == None
        Return
    EndIf

    If akNewLoc == LocCranberryWatogaLocation
        If PlayerKeywordWatoga.GetReference() != player
            PlayerKeywordWatoga.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocForestWadeAirportLocation
        If PlayerKeywordWadeAirport.GetReference() != player
            PlayerKeywordWadeAirport.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocSwampValleyGalleriaLocation
        If PlayerKeywordValleyGalleria.GetReference() != player
            PlayerKeywordValleyGalleria.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocToxicEasternRegionalPenLocation
        If PlayerKeywordEasternRegional.GetReference() != player
            PlayerKeywordEasternRegional.ForceRefTo(player)
        EndIf
    ElseIf akNewLoc == LocToxicWavyWillardsWaterparkLocation
        If PlayerKeywordWavyWillards.GetReference() != player
            PlayerKeywordWavyWillards.ForceRefTo(player)
        EndIf
    EndIf
EndEvent

; FO76 closed the repair objective from OnItemCrafted, which FO4 does not have.
; The tinker's-workbench recipe turns a broken tube into a normal one, so the
; player's remaining broken-tube count is the equivalent signal.
Event OnAliasInit()
    AddInventoryEventFilter(None)
    RefreshRepairProgress()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    RefreshRepairProgress()
EndEvent

Event OnItemRemoved(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
    RefreshRepairProgress()
EndEvent

Function RefreshRepairProgress()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || !owner.IsStageDone(300) || owner.IsStageDone(iTalkToWrenStage)
        Return
    EndIf
    ObjectReference player = GetReference()
    W05_Daily_R01_QuestScript_NEW techQuest = owner as W05_Daily_R01_QuestScript_NEW
    If player == None || techQuest == None || techQuest.W05_Daily_R01_TechBroken == None
        Return
    EndIf

    Int brokenTotal = techQuest.TechBroken
    If brokenTotal <= 0
        brokenTotal = 1
    EndIf
    Int brokenLeft = player.GetItemCount(techQuest.W05_Daily_R01_TechBroken)
    Int repaired = brokenTotal - brokenLeft
    If repaired < 0
        repaired = 0
    EndIf

    B21:QuestVariables questVariables = owner as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(BrokenTechCount, repaired as Float)
        questVariables.SetVariable(BrokenTechTotal, brokenTotal as Float)
    EndIf

    If brokenLeft <= 0
        owner.SetStage(iTalkToWrenStage)
    EndIf
EndFunction
