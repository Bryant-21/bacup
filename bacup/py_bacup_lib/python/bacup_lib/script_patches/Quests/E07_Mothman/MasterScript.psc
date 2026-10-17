; FO76's server ran this Equinox dialogue master. Its 141 cultist lines are gated on
; E07A_Mothman_Global_EventState: 0 idle, 1 event running, 2 the Wise Mothman is present after the ritual,
; 3 he has left. The event quest (Quests:E07_Mothman:QuestScript) starts this quest and reports start and end;
; each post-ritual phase lasts E07A_Mothman_Global_Timer_EventReset seconds.
Function EventStarted()
    CancelTimer(TimerID_ResetTime)
    SetEventState(1.0)
    RefCollectionAlias communed = GetAlias(17) as RefCollectionAlias
    If communed != None
        communed.RemoveAll()
    EndIf
EndFunction

Function EventEnded(Bool abCompleted)
    CancelTimer(TimerID_ResetTime)
    If abCompleted && WiseMothmanRef() != None
        WiseMothmanRef().Enable()
        SetEventState(2.0)
        StartResetTimer()
    Else
        DismissWiseMothman()
        SetEventState(0.0)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != TimerID_ResetTime || E07A_Mothman_Global_EventState == None
        Return
    EndIf
    If E07A_Mothman_Global_EventState.GetValue() == 2.0
        DismissWiseMothman()
        SetEventState(3.0)
        StartResetTimer()
    ElseIf E07A_Mothman_Global_EventState.GetValue() == 3.0
        SetEventState(0.0)
    EndIf
EndEvent

Function StartResetTimer()
    Float seconds = 600.0
    If E07A_Mothman_Global_Timer_EventReset != None && E07A_Mothman_Global_Timer_EventReset.GetValue() > 0.0
        seconds = E07A_Mothman_Global_Timer_EventReset.GetValue()
    EndIf
    StartTimer(seconds, TimerID_ResetTime)
EndFunction

ObjectReference Function WiseMothmanRef()
    If Actor_WiseMothman == None
        Return None
    EndIf
    Return Actor_WiseMothman.GetReference()
EndFunction

Function DismissWiseMothman()
    ObjectReference mothman = WiseMothmanRef()
    If mothman != None
        mothman.Disable()
    EndIf
EndFunction

Function SetEventState(Float afState)
    If E07A_Mothman_Global_EventState != None
        E07A_Mothman_Global_EventState.SetValue(afState)
    EndIf
EndFunction
