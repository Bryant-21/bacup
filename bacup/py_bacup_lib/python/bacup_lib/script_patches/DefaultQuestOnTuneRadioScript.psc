; Sets a stage when the player tunes the radio to this quest's station.
;
; Neither game raises an event when the player changes station, which is why
; the FO76 script carries `timerSeconds = 2.0` and `timerID = 1` and nothing
; else: it polled. Fallout 4 exposes the same reading through
; Game.IsPlayerRadioOn() / Game.GetPlayerRadioFrequency(), and the converted
; plugin still carries the transmitters — 90 placed refs with an XRDO frequency,
; including 0040D4E3 at 92.5 for Cheating Death — so this is the original shape
; rather than a substitute.
;
; The poll only runs while the stage is actually reachable, and it stops as soon
; as it is not, so a quest that is finished with the radio costs nothing.

Event OnQuestInit()
    RefreshRadioWatch()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshRadioWatch()
EndEvent

Event OnQuestShutdown()
    CancelTimer(timerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != timerID
        Return
    EndIf
    If !RadioWatchIsLive()
        Return
    EndIf
    If PlayerIsTunedToStation()
        Trace("tuned to " + RadioFrequency + "; setting stage " + StageToSet)
        SetStage(StageToSet)
        ; OnStageSet re-evaluates the guards, which now refuse, so the poll ends.
        Return
    EndIf
    StartTimer(timerSeconds, timerID)
EndEvent

Function RefreshRadioWatch()
    CancelTimer(timerID)
    If RadioWatchIsLive()
        StartTimer(timerSeconds, timerID)
    EndIf
EndFunction

; The prerequisite has run, the turn-off stage has not, and the stage this
; script owns is still unset. `TurnOffStage` equal to `StageToSet` is the
; shipped shape on Cheating Death, so the second condition alone must not be
; relied on to stop the loop.
Bool Function RadioWatchIsLive()
    If StageToSet < 0 || !IsRunning()
        Return False
    EndIf
    If IsStageDone(StageToSet)
        Return False
    EndIf
    If PrereqStage >= 0 && !IsStageDone(PrereqStage)
        Return False
    EndIf
    If TurnOffStage >= 0 && IsStageDone(TurnOffStage)
        Return False
    EndIf
    Return True
EndFunction

Bool Function PlayerIsTunedToStation()
    If !Game.IsPlayerRadioOn()
        Return False
    EndIf
    ; Frequencies are authored to one decimal place; compare with a tolerance
    ; rather than for equality so a float round-trip cannot miss the station.
    Return Math.Abs(Game.GetPlayerRadioFrequency() - RadioFrequency) < 0.05
EndFunction

Function Trace(String asMessage)
    If ShowTraces
        Debug.Trace("DefaultQuestOnTuneRadioScript: " + asMessage)
    EndIf
EndFunction
