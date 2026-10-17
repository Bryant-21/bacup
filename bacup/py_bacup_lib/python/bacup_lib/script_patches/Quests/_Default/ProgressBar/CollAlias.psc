Quests:_Default:ProgressBar:MasterScript Function GetProgressBar()
    If ProgressBarQuest == None
        ProgressBarQuest = GetOwningQuest()
    EndIf
    If ProgressBar == None && ProgressBarQuest != None
        ProgressBar = ProgressBarQuest as Quests:_Default:ProgressBar:MasterScript
    EndIf
    Return ProgressBar
EndFunction

; No ValidStages means always valid; otherwise any one open range is enough.
Bool Function CanContribute()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || GetProgressBar() == None
        Return False
    EndIf
    If ValidStages == None || ValidStages.Length == 0
        Return True
    EndIf
    Int index = 0
    While index < ValidStages.Length
        StageRange range = ValidStages[index]
        If range != None && (range.StartStage < 0 || owningQuest.IsStageDone(range.StartStage)) && (range.EndStage < 0 || !owningQuest.IsStageDone(range.EndStage))
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Float Function ContributionMultiplier()
    If WearingKeyword != None && BonusMultiplier > 0.0 && Game.GetPlayer().WornHasKeyword(WearingKeyword)
        Return BonusMultiplier
    EndIf
    Return 1.0
EndFunction

Function ContributeProgress(Float afContributions, ObjectReference akSource)
    Quests:_Default:ProgressBar:MasterScript masterScript = GetProgressBar()
    If masterScript == None || afContributions <= 0.0
        Return
    EndIf
    masterScript.ModProgress(ModPercentage * afContributions * ContributionMultiplier())
    If SuccessSound != None && akSource != None
        SuccessSound.Play(akSource)
    EndIf
EndFunction

Function PlayErrorFeedback(ObjectReference akSource)
    If ErrorSound != None && akSource != None
        ErrorSound.Play(akSource)
    EndIf
    If ErrorMessage != None
        ErrorMessage.Show()
    EndIf
EndFunction

Event OnAliasShutdown()
    ProgressBar = None
    ProgressBarQuest = None
EndEvent
