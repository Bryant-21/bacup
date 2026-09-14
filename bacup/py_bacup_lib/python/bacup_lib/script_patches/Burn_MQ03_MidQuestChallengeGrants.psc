; Fallout 4 has no CHAL record and no Challenge Menu, so ChallengeSetOne and
; ChallengeSetTwo arrive as null arrays and are deliberately never read here.
; The two gates are re-expressed as a cumulative count of completed Burning
; Springs bounty hunts, which is the one challenge in either set whose subject
; survives conversion as an FO4-observable record:
; Challenge_BurnBounty_CompletedGruntHunt -> QUST Burn_BountyHunt_GruntHunt,
; whose repaired stage 9000 already increments this actor value.
; The per-phase thresholds are the shipped StageCompletionTargets globals,
; unchanged.

Actor Function ChallengePlayerReference()
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf
    Return PlayerRef
EndFunction

ActorValue Function CompletedBountyCounter()
    Return Game.GetFormFromFile(0x00829BB6, "SeventySix.esm") as ActorValue
EndFunction

Int Function CompletedBountyCount()
    Actor player = ChallengePlayerReference()
    ActorValue counter = CompletedBountyCounter()
    If player == None || counter == None
        Return 0
    EndIf
    Return player.GetValue(counter) as Int
EndFunction

Int Function CumulativeTargetFor(Int aiGroup)
    If StageCompletionTargets == None
        Return 0
    EndIf
    Int total = 0
    Int index = 0
    While index <= aiGroup && index < StageCompletionTargets.Length
        If StageCompletionTargets[index] != None
            total += StageCompletionTargets[index].GetValue() as Int
        EndIf
        index += 1
    EndWhile
    Return total
EndFunction

Bool Function GroupIsOpen(Int aiGroup)
    If StageValues == None || aiGroup >= StageValues.Length
        Return False
    EndIf
    If CompletedStageValues == None || aiGroup >= CompletedStageValues.Length
        Return False
    EndIf
    Return IsStageDone(StageValues[aiGroup]) && !IsStageDone(CompletedStageValues[aiGroup])
EndFunction

Function RecordGroupProgress(Int aiGroup, Int aiEarned)
    If StagedProgress == None || aiGroup >= StagedProgress.Length
        Return
    EndIf
    Int floor = 0
    If aiGroup > 0
        floor = CumulativeTargetFor(aiGroup - 1)
    EndIf
    Int credited = aiEarned - floor
    If credited < 0
        credited = 0
    EndIf
    StagedProgress[aiGroup] = credited
EndFunction

Function UnlockGroup(Int aiGroup)
    Actor player = ChallengePlayerReference()
    If player != None && StageUnlocked != None && aiGroup < StageUnlocked.Length && StageUnlocked[aiGroup] != None
        player.SetValue(StageUnlocked[aiGroup], 1.0)
    EndIf
    SetStage(CompletedStageValues[aiGroup])
EndFunction

Function EvaluateChallengeProgress()
    Int earned = CompletedBountyCount()
    Int groupIndex = 0
    While groupIndex < ChallengeGroupAmount
        If GroupIsOpen(groupIndex)
            RecordGroupProgress(groupIndex, earned)
            If earned >= CumulativeTargetFor(groupIndex)
                UnlockGroup(groupIndex)
            EndIf
        EndIf
        groupIndex += 1
    EndWhile
    ScheduleNextEvaluation()
EndFunction

Function ScheduleNextEvaluation()
    Int groupIndex = 0
    While groupIndex < ChallengeGroupAmount
        If GroupIsOpen(groupIndex)
            StartTimer(5.0, 1)
            Return
        EndIf
        groupIndex += 1
    EndWhile
EndFunction

Event OnQuestInit()
    PlayerRef = Game.GetPlayer()
    If PlayerRef != None
        RegisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")
    EndIf
    EvaluateChallengeProgress()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    EvaluateChallengeProgress()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        EvaluateChallengeProgress()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    EvaluateChallengeProgress()
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    If PlayerRef != None
        UnregisterForRemoteEvent(PlayerRef, "OnPlayerLoadGame")
    EndIf
EndEvent
