; Keep the tower retryable unless its repeatable Story Manager quest starts.

Event OnActivate(ObjectReference akActionRef)
    Bool questStarted

    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If ThisTowerValue == None || LookoutTowerQuestKeyword == None
        Return
    EndIf

    If akActionRef.GetValue(ThisTowerValue) > 0.0
        Return
    EndIf

    questStarted = LookoutTowerQuestKeyword.SendStoryEventAndWait(None, Self, akActionRef)
    If !questStarted
        Return
    EndIf

    akActionRef.SetValue(ThisTowerValue, 1.0)
    Utility.Wait(0.5)
    If SurveySound != None
        SurveySound.Play(akActionRef)
    EndIf
EndEvent
