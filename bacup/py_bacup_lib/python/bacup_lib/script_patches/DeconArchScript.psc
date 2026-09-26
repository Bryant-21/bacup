Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        B21PlayerInside = True
        TreatLocalPlayer()
        StartTimer(1.0, CONST_DeconArchTimerID)
    EndIf
EndEvent

Function TreatLocalPlayer()
    If B21PlayerInside && (Self as Default2StateActivator).isOpen && DeconArchSpell != None
        DeconArchSpell.Cast(Self, Game.GetPlayer())
    EndIf
EndFunction

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        B21PlayerInside = False
        CancelTimer(CONST_DeconArchTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != CONST_DeconArchTimerID
        Parent.OnTimer(aiTimerID)
        Return
    EndIf
    If !Is3DLoaded()
        Return
    EndIf
    If GetTriggerObjectCount() == 0
        B21PlayerInside = False
    EndIf
    If B21PlayerInside
        TreatLocalPlayer()
        StartTimer(1.0, CONST_DeconArchTimerID)
    EndIf
EndEvent

Event OnLoad()
    SetDefaultState()
    If B21PlayerInside
        StartTimer(1.0, CONST_DeconArchTimerID)
    EndIf
EndEvent

Event OnUnload()
    CancelTimer(CONST_DeconArchTimerID)
EndEvent

Event OnReset()
    B21PlayerInside = False
    CancelTimer(CONST_DeconArchTimerID)
    SetDefaultState()
EndEvent

Function SetDefaultState()
    Default2StateActivator archState = Self as Default2StateActivator
    If !archState.shouldSetDefaultState
        Return
    EndIf
    If archState.isOpen
        PlayAnimation(archState.startOpenAnim)
        archState.myState = 0
    Else
        PlayAnimation(archState.closeAnim)
        archState.myState = 1
    EndIf
    If archState.TwoStateCollisionKeyword != None
        If archState.isOpen == archState.InvertCollision
            EnableLinkChain(archState.TwoStateCollisionKeyword)
        Else
            DisableLinkChain(archState.TwoStateCollisionKeyword)
        EndIf
    EndIf
EndFunction
