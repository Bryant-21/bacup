; The attract scene begins with the quest and its phases are conditioned on
; stage 20 not being done. The final phase schedules a repeat so the terminal
; keeps calling players until construction starts.

Function Fragment_Phase_01_End()
EndFunction

Function Fragment_Phase_02_End()
EndFunction

Function Fragment_Phase_03_End()
    If ENz04_AttractStillWanted()
        StartTimer(20.0, 1)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1 && ENz04_AttractStillWanted() && !IsPlaying()
        Start()
    EndIf
EndEvent

Bool Function ENz04_AttractStillWanted()
    Quest owner = GetOwningQuest()
    If owner == None || !owner.IsRunning() || owner.IsStopping()
        Return False
    EndIf
    Return !owner.IsStageDone(20) && !owner.IsStageDone(175) && !owner.IsStageDone(180) && !owner.IsStageDone(195)
EndFunction
