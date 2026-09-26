; RE_ObjectSC01_MoM turns off REScript's unload cleanup, and its only shutdown stage
; is 1000, whose fragment re-arms the trigger. This alias is therefore the encounter's
; cleanup: once the corpse has unloaded, stop the encounter unless a MoM00 instance
; still holds the corpse through its keyword, in which case check again later.
Event OnUnload()
    StartTimer(CONST_CleanupTimerDelay, CONST_CleanupTimerID)
EndEvent

Event OnLoad()
    CancelTimer(CONST_CleanupTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != CONST_CleanupTimerID
        Return
    EndIf
    Quest encounter = GetOwningQuest()
    If encounter == None || !encounter.IsRunning() || encounter.IsStageDone(1000)
        Return
    EndIf
    ObjectReference corpse = GetReference()
    If corpse != None
        If corpse.Is3DLoaded()
            Return
        EndIf
        If MoM00ActiveCorpseKeyword != None && corpse.HasKeyword(MoM00ActiveCorpseKeyword)
            StartTimer(CONST_CleanupTimerDelay, CONST_CleanupTimerID)
            Return
        EndIf
    EndIf
    encounter.SetStage(1000)
EndEvent
