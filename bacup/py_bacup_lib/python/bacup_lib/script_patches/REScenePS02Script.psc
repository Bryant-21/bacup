; RE_ScenePS02 Miss Nanny storytime. Talking to her starts storytime (100) with a random StoryScenes scene; the story
; ending is snack time (300, B21:QuestRewards pays it); hitting her mid-story interrupts it (200).
Event OnQuestInit()
    ObjectReference nanny = MissNanny.GetReference()
    If nanny != None
        RegisterForRemoteEvent(nanny, "OnActivate")
        RegisterForHitEvent(nanny)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100 && StoryScenes != None && StoryScenes.Length > 0
        Scene story = StoryScenes[Utility.RandomInt(0, StoryScenes.Length - 1)]
        If story != None
            RegisterForRemoteEvent(story, "OnEnd")
            story.Start()
        EndIf
    EndIf
EndEvent

Event Scene.OnEnd(Scene akSender)
    UnregisterForRemoteEvent(akSender, "OnEnd")
    If IsStageDone(100) && !IsStageDone(200) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    If IsStageDone(100) && !IsStageDone(200) && !IsStageDone(300)
        Int index = 0
        While StoryScenes != None && index < StoryScenes.Length
            If StoryScenes[index] != None && StoryScenes[index].IsPlaying()
                UnregisterForRemoteEvent(StoryScenes[index], "OnEnd")
                StoryScenes[index].Stop()
            EndIf
            index += 1
        EndWhile
        SetStage(200)
    ElseIf !IsStageDone(300) && !IsStageDone(200)
        RegisterForHitEvent(akTarget)
    EndIf
EndEvent
