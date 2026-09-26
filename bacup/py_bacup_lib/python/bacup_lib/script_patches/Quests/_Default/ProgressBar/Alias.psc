Quests:_Default:ProgressBar:MasterScript Function GetProgressBar()
    If ProgressBarQuest == None
        ProgressBarQuest = GetOwningQuest()
    EndIf
    If ProgressBar == None && ProgressBarQuest != None
        ProgressBar = ProgressBarQuest as Quests:_Default:ProgressBar:MasterScript
    EndIf
    Return ProgressBar
EndFunction

Bool Function CanContribute()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return False
    EndIf
    If StartStage >= 0 && !owningQuest.IsStageDone(StartStage)
        Return False
    EndIf
    If EndStage >= 0 && owningQuest.IsStageDone(EndStage)
        Return False
    EndIf
    Return GetProgressBar() != None
EndFunction

Function ContributeProgress(Float afContributions)
    Quests:_Default:ProgressBar:MasterScript masterScript = GetProgressBar()
    If masterScript == None || afContributions <= 0.0
        Return
    EndIf
    masterScript.ModProgress(ModPercentage * afContributions)
    ObjectReference soundSource = GetReference()
    If SuccessSound != None && soundSource != None
        SuccessSound.Play(soundSource)
    EndIf
EndFunction

Function PlayErrorFeedback()
    ObjectReference soundSource = GetReference()
    If ErrorSound != None && soundSource != None
        ErrorSound.Play(soundSource)
    EndIf
    If ErrorMessage != None
        ErrorMessage.Show()
    EndIf
EndFunction

Event OnAliasShutdown()
    ProgressBar = None
    ProgressBarQuest = None
EndEvent
