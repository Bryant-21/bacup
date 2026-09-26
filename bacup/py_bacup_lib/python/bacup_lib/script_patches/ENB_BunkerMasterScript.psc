Function RegisterDeconTrigger()
    ReferenceAlias triggerAlias = GetAlias(5) as ReferenceAlias
    If triggerAlias != None && triggerAlias.GetReference() != None
        RegisterForRemoteEvent(triggerAlias.GetReference(), "OnTriggerEnter")
        RegisterForRemoteEvent(triggerAlias.GetReference(), "OnTriggerLeave")
    EndIf
EndFunction

Event OnQuestInit()
    InitializeDeconController()
EndEvent

ObjectReference[] Function CopyDeconReferences(RefCollectionAlias akCollection)
    ObjectReference[] references = new ObjectReference[0]
    If akCollection != None
        Int index = 0
        While index < akCollection.GetCount()
            ObjectReference ref = akCollection.GetAt(index)
            If ref != None
                references.Add(ref)
            EndIf
            index += 1
        EndWhile
    EndIf
    Return references
EndFunction

Function InitializeDeconController()
    If !IsRunning()
        Return
    EndIf
    If !bDeconActive
        B21DeconArches = CopyDeconReferences(DeconArches)
        B21DeconDoors = CopyDeconReferences(DeconDoors)
    EndIf
    RegisterDeconTrigger()
EndFunction

Function SetDeconArchesOpen(Bool abOpen)
    If B21DeconArches == None
        Return
    EndIf
    Int index = 0
    While index < B21DeconArches.Length
        Default2StateActivator arch = B21DeconArches[index] as Default2StateActivator
        If arch != None
            arch.AllowInterrupt = True
            arch.SetOpenNoWait(abOpen)
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetDeconDoorsOpen(Bool abOpen)
    If B21DeconDoors == None
        Return
    EndIf
    Int index = 0
    While index < B21DeconDoors.Length
        Default2StateActivator gate = B21DeconDoors[index] as Default2StateActivator
        If gate != None
            gate.AllowInterrupt = True
            gate.BlockActivation(!abOpen)
            gate.SetOpenNoWait(abOpen)
        EndIf
        index += 1
    EndWhile
EndFunction

Function FinishDecon()
    CancelTimer(iDoorCloseTimerID)
    CancelTimer(iDeconTimerID)
    bDeconActive = False
    PlayerHolding = None
    SetDeconArchesOpen(False)
    SetDeconDoorsOpen(True)
EndFunction

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
    ReferenceAlias triggerAlias = GetAlias(5) as ReferenceAlias
    If !IsRunning() || bDeconActive || akActionRef != Game.GetPlayer() \
        || triggerAlias == None || akSender != triggerAlias.GetReference()
        Return
    EndIf
    If B21DeconArches == None || B21DeconArches.Length == 0 || B21DeconDoors == None || B21DeconDoors.Length == 0
        FinishDecon()
        Return
    EndIf
    bDeconActive = True
    PlayerHolding = akActionRef as Actor
    StartTimer(iDoorCloseTimerLength as Float, iDoorCloseTimerID)
EndEvent

Event ObjectReference.OnTriggerLeave(ObjectReference akSender, ObjectReference akActionRef)
    ReferenceAlias triggerAlias = GetAlias(5) as ReferenceAlias
    If bDeconActive && akActionRef == PlayerHolding && triggerAlias != None && akSender == triggerAlias.GetReference()
        FinishDecon()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning() || !bDeconActive || PlayerHolding == None
        Return
    EndIf
    If aiTimerID == iDoorCloseTimerID
        SetDeconDoorsOpen(False)
        SetDeconArchesOpen(True)
        StartTimer(iDeconTimerLength as Float, iDeconTimerID)
    ElseIf aiTimerID == iDeconTimerID
        FinishDecon()
    EndIf
EndEvent

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
    FinishDecon()
EndEvent
