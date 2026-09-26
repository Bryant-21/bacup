Function ReconcileFurniture()
    myPlayerRef = Game.GetPlayer()
    If myPlayerRef == None || AtlasLoc == None || BS01_MQ02_Invention == None || BS01_MQ07_Over == None
        Return
    EndIf
    mySocialSpaceLoc = AtlasLoc.GetLocation()
    If mySocialSpaceLoc == None || !myPlayerRef.IsInLocation(mySocialSpaceLoc)
        Return
    EndIf
    If Alias_Valdez != None
        myValdezRef = Alias_Valdez.GetActorReference()
    EndIf
    If Alias_UltraciteBatteryFurniture != None
        UBFurnitureRef = Alias_UltraciteBatteryFurniture.GetReference()
    EndIf
    If Alias_EmptyTable != None
        EmptyTableRef = Alias_EmptyTable.GetReference()
    EndIf
    If Alias_ClipboardFurniture != None
        ClipboardFurnitureRef = Alias_ClipboardFurniture.GetReference()
    EndIf
    Bool studyingBattery = BS01_MQ02_Invention.IsCompleted() && !BS01_MQ07_Over.IsCompleted()
    If BS01_MQ07_Over_QuestActiveKeyword != None && myPlayerRef.HasKeyword(BS01_MQ07_Over_QuestActiveKeyword)
        studyingBattery = False
    EndIf
    SetFurnitureEnabled(UBFurnitureRef, studyingBattery)
    SetFurnitureEnabled(EmptyTableRef, !studyingBattery)
    SetFurnitureEnabled(ClipboardFurnitureRef, !studyingBattery)
    If myValdezRef != None
        myValdezRef.EvaluatePackage()
    EndIf
EndFunction

Function SetFurnitureEnabled(ObjectReference furnitureRef, Bool enabled)
    If furnitureRef != None
        If enabled
            furnitureRef.EnableNoWait()
        Else
            furnitureRef.DisableNoWait()
        EndIf
    EndIf
EndFunction

Event OnAliasInit()
    myPlayerRef = Game.GetPlayer()
    If myPlayerRef != None
        RegisterForRemoteEvent(myPlayerRef, "OnLocationChange")
    EndIf
    If BS01_MQ02_Invention != None
        RegisterForRemoteEvent(BS01_MQ02_Invention, "OnStageSet")
    EndIf
    If BS01_MQ07_Over != None
        RegisterForRemoteEvent(BS01_MQ07_Over, "OnStageSet")
    EndIf
    ReconcileFurniture()
EndEvent

Event OnPlayerLoadGame()
    OnAliasInit()
EndEvent

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender == myPlayerRef
        ReconcileFurniture()
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == BS01_MQ02_Invention || akSender == BS01_MQ07_Over
        StartTimer(0.1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 0
        ReconcileFurniture()
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer()
    UnregisterForAllEvents()
EndEvent
