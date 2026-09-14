Event OnQuestInit()
    ReconcileRuntimeRegistrations()

    If !IsStageDone(10)
        SetStage(10)
    EndIf
EndEvent

Function ReconcileRuntimeRegistrations()
    If !IsRunning() || IsCompleted()
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        UnregisterForRemoteEvent(playerRef, "OnItemEquipped")
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf

    UnregisterAliasEvent(1, "OnActivate")
    UnregisterAliasEvent(2, "OnTriggerEnter")
    UnregisterAliasEvent(19, "OnTriggerEnter")
    UnregisterAliasEvent(20, "OnActivate")
    UnregisterAliasEvent(53, "OnTriggerEnter")
    UnregisterAliasEvent(54, "OnTriggerEnter")
    UnregisterAliasEvent(55, "OnTriggerEnter")
    UnregisterTerminalEvent(21)
    UnregisterTerminalEvent(47)
    UnregisterTerminalEvent(48)
    UnregisterTerminalEvent(49)
    UnregisterHolotapePlay(HolotapeNari)
    UnregisterHolotapePlay(HolotapeRandy)
    UnregisterHolotapePlay(HolotapeLucy)

    RemoveAllInventoryEventFilters()
    RegisterTrackedItemFilters()

    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnItemAdded")
        RegisterForRemoteEvent(playerRef, "OnItemEquipped")
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf

    RegisterAliasEvent(1, "OnActivate")
    RegisterAliasEvent(2, "OnTriggerEnter")
    RegisterAliasEvent(19, "OnTriggerEnter")
    RegisterAliasEvent(20, "OnActivate")
    RegisterAliasEvent(53, "OnTriggerEnter")
    RegisterAliasEvent(54, "OnTriggerEnter")
    RegisterAliasEvent(55, "OnTriggerEnter")
    RegisterTerminalEvent(21)
    RegisterTerminalEvent(47)
    RegisterTerminalEvent(48)
    RegisterTerminalEvent(49)
    RegisterHolotapePlay(HolotapeNari)
    RegisterHolotapePlay(HolotapeRandy)
    RegisterHolotapePlay(HolotapeLucy)
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileRuntimeRegistrations()
    EndIf
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akSender != Game.GetPlayer() || aiItemCount <= 0
        Return
    EndIf

    If IsStageDone(300)
        SetStageForAliasForm(akBaseItem, SignalBooster01, 400)
        SetStageForAliasForm(akBaseItem, SignalBooster03, 400)
    EndIf
    SetStageForAliasForm(akBaseItem, RandyBeacon, 900)
    SetStageForAliasForm(akBaseItem, HolotapeNari, 910)
    SetStageForAliasForm(akBaseItem, HolotapeRandy, 920)
    SetStageForAliasForm(akBaseItem, NariHazmatSuit, 930)
    SetStageForAliasForm(akBaseItem, NariIDCard, 1030)
    If IsStageDone(1200)
        SetStageForAliasForm(akBaseItem, HolotapeLucy, 1300)
    EndIf
EndEvent

Event Actor.OnItemEquipped(Actor akSender, Form akBaseObject, ObjectReference akReference)
    If akSender != Game.GetPlayer()
        Return
    EndIf

    SetStageForAliasForm(akBaseObject, HolotapeNari, 1010)
    SetStageForAliasForm(akBaseObject, HolotapeRandy, 1020)
    SetStageForAliasForm(akBaseObject, HolotapeLucy, 2000)
EndEvent

Event ObjectReference.OnHolotapePlay(ObjectReference akSender, ObjectReference akTerminalRef)
    If akSender == None
        Return
    EndIf

    Form playedHolotape = akSender.GetBaseObject()
    SetStageForAliasForm(playedHolotape, HolotapeNari, 1010)
    SetStageForAliasForm(playedHolotape, HolotapeRandy, 1020)
    SetStageForAliasForm(playedHolotape, HolotapeLucy, 2000)
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If akSender == Randy.GetReference()
        If IsStageDone(300) && !IsStageDone(500) && !IsStageDone(350)
            SetStage(350)
        ElseIf IsStageDone(200) && !IsStageDone(300) && !IsStageDone(250)
            SetStage(250)
        EndIf
    ElseIf IsAliasReference(akSender, 20) && !IsStageDone(1110)
        SetStage(1110)
    EndIf
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    ReferenceAlias hardballTrigger = GetAlias(2) as ReferenceAlias
    ReferenceAlias sewerTrigger = GetAlias(19) as ReferenceAlias
    If (IsAliasReference(akSender, 53) || IsAliasReference(akSender, 54) || IsAliasReference(akSender, 55)) && !IsStageDone(100) && !IsStageDone(50)
        SetStage(50)
    ElseIf hardballTrigger != None && akSender == hardballTrigger.GetReference() && !IsStageDone(150)
        SetStage(150)
    ElseIf sewerTrigger != None && akSender == sewerTrigger.GetReference() && IsStageDone(1100) && !IsStageDone(1200)
        SetStage(1200)
    EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID != 1
        Return
    EndIf

    If IsAliasReference(akTerminalRef, 21) && !IsStageDone(450)
        SetStage(450)
    ElseIf (IsAliasReference(akTerminalRef, 47) || IsAliasReference(akTerminalRef, 48) || IsAliasReference(akTerminalRef, 49)) && !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnItemAdded")
        UnregisterForRemoteEvent(playerRef, "OnItemEquipped")
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    UnregisterAliasEvent(1, "OnActivate")
    UnregisterAliasEvent(2, "OnTriggerEnter")
    UnregisterAliasEvent(19, "OnTriggerEnter")
    UnregisterAliasEvent(20, "OnActivate")
    UnregisterAliasEvent(53, "OnTriggerEnter")
    UnregisterAliasEvent(54, "OnTriggerEnter")
    UnregisterAliasEvent(55, "OnTriggerEnter")
    UnregisterTerminalEvent(21)
    UnregisterTerminalEvent(47)
    UnregisterTerminalEvent(48)
    UnregisterTerminalEvent(49)
    UnregisterHolotapePlay(HolotapeNari)
    UnregisterHolotapePlay(HolotapeRandy)
    UnregisterHolotapePlay(HolotapeLucy)
    RemoveAllInventoryEventFilters()
EndEvent

Function RegisterTrackedItemFilters()
    Form[] inventoryFilters = new Form[8]
    inventoryFilters[0] = GetAliasBaseObject(SignalBooster01)
    inventoryFilters[1] = GetAliasBaseObject(SignalBooster03)
    inventoryFilters[2] = GetAliasBaseObject(RandyBeacon)
    inventoryFilters[3] = GetAliasBaseObject(HolotapeNari)
    inventoryFilters[4] = GetAliasBaseObject(HolotapeRandy)
    inventoryFilters[5] = GetAliasBaseObject(NariHazmatSuit)
    inventoryFilters[6] = GetAliasBaseObject(NariIDCard)
    inventoryFilters[7] = GetAliasBaseObject(HolotapeLucy)

    Int index = 0
    While index < inventoryFilters.Length
        Form inventoryFilter = inventoryFilters[index]
        If inventoryFilter != None && inventoryFilters.Find(inventoryFilter) == index
            AddInventoryEventFilter(inventoryFilter)
        EndIf
        index += 1
    EndWhile
EndFunction

Form Function GetAliasBaseObject(ReferenceAlias akItemAlias)
    ObjectReference itemRef = None
    If akItemAlias != None
        itemRef = akItemAlias.GetReference()
    EndIf
    If itemRef != None
        Return itemRef.GetBaseObject()
    EndIf
    Return None
EndFunction

Function RegisterAliasEvent(Int aiAliasId, String asEventName)
    ReferenceAlias targetAlias = GetAlias(aiAliasId) as ReferenceAlias
    If targetAlias != None && targetAlias.GetReference() != None
        RegisterForRemoteEvent(targetAlias.GetReference(), asEventName)
    EndIf
EndFunction

Function RegisterTerminalEvent(Int aiAliasId)
    ReferenceAlias targetAlias = GetAlias(aiAliasId) as ReferenceAlias
    ObjectReference targetRef = None
    Terminal targetTerminal = None
    If targetAlias != None
        targetRef = targetAlias.GetReference()
    EndIf
    If targetRef != None
        targetTerminal = targetRef.GetBaseObject() as Terminal
    EndIf
    If targetTerminal != None
        RegisterForRemoteEvent(targetTerminal, "OnMenuItemRun")
    EndIf
EndFunction

Function UnregisterAliasEvent(Int aiAliasId, String asEventName)
    ReferenceAlias targetAlias = GetAlias(aiAliasId) as ReferenceAlias
    If targetAlias != None && targetAlias.GetReference() != None
        UnregisterForRemoteEvent(targetAlias.GetReference(), asEventName)
    EndIf
EndFunction

Function UnregisterTerminalEvent(Int aiAliasId)
    ReferenceAlias targetAlias = GetAlias(aiAliasId) as ReferenceAlias
    ObjectReference targetRef = None
    Terminal targetTerminal = None
    If targetAlias != None
        targetRef = targetAlias.GetReference()
    EndIf
    If targetRef != None
        targetTerminal = targetRef.GetBaseObject() as Terminal
    EndIf
    If targetTerminal != None
        UnregisterForRemoteEvent(targetTerminal, "OnMenuItemRun")
    EndIf
EndFunction

Function RegisterHolotapePlay(ReferenceAlias akHolotapeAlias)
    If akHolotapeAlias != None && akHolotapeAlias.GetReference() != None
        RegisterForRemoteEvent(akHolotapeAlias.GetReference(), "OnHolotapePlay")
    EndIf
EndFunction

Function UnregisterHolotapePlay(ReferenceAlias akHolotapeAlias)
    If akHolotapeAlias != None && akHolotapeAlias.GetReference() != None
        UnregisterForRemoteEvent(akHolotapeAlias.GetReference(), "OnHolotapePlay")
    EndIf
EndFunction

Function SetStageForAliasForm(Form akBaseItem, ReferenceAlias akItemAlias, Int aiStage)
    ObjectReference itemRef = None
    If akItemAlias != None
        itemRef = akItemAlias.GetReference()
    EndIf
    If itemRef != None && akBaseItem == itemRef.GetBaseObject() && !IsStageDone(aiStage)
        SetStage(aiStage)
    EndIf
EndFunction

Bool Function IsAliasReference(ObjectReference akReference, Int aiAliasId)
    ReferenceAlias targetAlias = GetAlias(aiAliasId) as ReferenceAlias
    Return targetAlias != None && akReference == targetAlias.GetReference()
EndFunction
