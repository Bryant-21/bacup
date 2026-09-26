Event OnQuestInit()
    If TutorialEventActive != None
        TutorialEventActive.SetValue(1.0)
    EndIf
EndEvent

Event OnQuestShutdown()
    If TutorialEventActive != None
        TutorialEventActive.SetValue(0.0)
    EndIf
EndEvent
