Event OnAliasInit()
    SyncAccessState()
EndEvent

; The daily runs again the next in-game day. Without re-syncing on reset the
; script stays parked in "terminalaccessed" from the previous run and the
; terminal never hands out stage 50 again.
Event OnAliasReset()
    SyncAccessState()
EndEvent

Function SyncAccessState()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.GetStage() >= TerminalAccessedStage
        GoToState("terminalaccessed")
    Else
        GoToState("init")
    EndIf
EndFunction

State init
    Event OnActivate(ObjectReference akActivator)
        If akActivator != Game.GetPlayer()
            Return
        EndIf

        Quest owningQuest = GetOwningQuest()
        If owningQuest == None || !owningQuest.IsRunning()
            Return
        EndIf
        If owningQuest.GetStage() < TerminalAccessedStage
            owningQuest.SetStage(TerminalAccessedStage)
        EndIf
        GoToState("terminalaccessed")
    EndEvent
EndState

State terminalaccessed
    Event OnActivate(ObjectReference akActivator)
    EndEvent
EndState
