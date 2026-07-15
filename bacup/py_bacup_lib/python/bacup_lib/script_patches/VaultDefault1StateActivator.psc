Function InitializeLocalState()
    NetState_Waiting = 2
    NetState_Active = 1
    If PlayAnim == ""
        PlayAnim = "Play01"
    EndIf
    If PlayAnimEventName == ""
        PlayAnimEventName = "Done"
    EndIf
EndFunction

Function SetLocalActive(Bool active = True)
    InitializeLocalState()
    If active
        If GetState() != "Active"
            GoToState("Active")
        EndIf
    Else
        GoToState("Waiting")
    EndIf
EndFunction

Function UpdateNetworkState()
    InitializeLocalState()
    If GetState() == "Active"
        If Is3DLoaded() && PlayAnim != ""
            PlayAnimation(PlayAnim)
        EndIf
    Else
        GoToState("Waiting")
    EndIf
EndFunction

Event OnInit()
    InitializeLocalState()
    GoToState("Waiting")
EndEvent

Event OnLoad()
    UpdateNetworkState()
EndEvent

Event OnSimpleNetworkStateSet()
    UpdateNetworkState()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If GetState() == "Waiting"
        SetLocalActive(True)
    EndIf
EndEvent
