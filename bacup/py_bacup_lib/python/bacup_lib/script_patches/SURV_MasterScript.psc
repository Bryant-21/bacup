Event OnQuestInit()
    DispatchTutorialStoryEvent()
EndEvent

Event OnTimerGameTime(Int aiTimerID)
    If aiTimerID == 1
        DispatchTutorialStoryEvent()
    EndIf
EndEvent

; SURV_TutorialStart is the story-manager event that starts the tutorial quest.
; FO76 dispatched it server-side; SURV_Master is StartGameEnabled, so quest init
; is the single-player stand-in.
Function DispatchTutorialStoryEvent()
    If SURV_TutorialStart == None
        Return
    EndIf
    ObjectReference playerRef = Game.GetPlayer()
    If playerRef == None
        StartTimerGameTime(1.0, 1)
        Return
    EndIf
    SURV_TutorialStart.SendStoryEventAndWait(None, playerRef, playerRef)
EndFunction
