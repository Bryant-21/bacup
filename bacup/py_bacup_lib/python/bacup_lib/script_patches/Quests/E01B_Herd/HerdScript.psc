Event OnAliasInit()
    brahminRef = None
    CancelTimer(4986)
    If HerdingPerk != None
        RegisterForRemoteEvent(HerdingPerk, "OnEntryRun")
    EndIf
EndEvent

; BrahminHerdingPerk has no fragment in FO76 either; its Herd activate choice is observed through OnEntryRun.
Event Perk.OnEntryRun(Perk akSender, Int auiEntryID, ObjectReference akTarget, Actor akOwner)
    If akSender != HerdingPerk || akTarget == None || akTarget != GetReference()
        Return
    EndIf
    HerdBrahmin(akOwner)
EndEvent

Function HerdBrahmin(Actor akHerder)
    Quest owningQuest = GetOwningQuest()
    brahminRef = GetActorReference()
    If akHerder == None || akHerder != Game.GetPlayer() || brahminRef == None || brahminRef.IsDead()
        Return
    EndIf
    If owningQuest == None || !owningQuest.IsRunning() || owningQuest.IsCompleted() || owningQuest.IsStopping()
        Return
    EndIf
    If E01B_Herd_ScaredBrahminKeyword != None && brahminRef.HasKeyword(E01B_Herd_ScaredBrahminKeyword)
        Return
    EndIf

    If BellSound != None
        BellSound.Play(brahminRef)
    EndIf
    If E01B_Herd_PerkCooldownSpell != None
        E01B_Herd_PerkCooldownSpell.Cast(brahminRef, brahminRef)
    EndIf

    If !owningQuest.IsStageDone(Stage_EscortStart)
        owningQuest.SetStage(Stage_EscortStart)
        Return
    EndIf
    If IsStrayBrahmin(owningQuest) && owningQuest.IsStageDone(Stage_Brahmin3RanOffroad) && !owningQuest.IsStageDone(Stage_Brahmin3HerdedOffroad)
        owningQuest.SetStage(Stage_Brahmin3HerdedOffroad)
    EndIf
    ApplyScaredKeyword(BrahminSpeedBoostDuration)
EndFunction

Bool Function IsStrayBrahmin(Quest akOwningQuest)
    Quests:E01B_Herd:QuestScript eventScript = akOwningQuest as Quests:E01B_Herd:QuestScript
    Return eventScript != None && eventScript.Alias_Actor_Brahmin_03 == Self
EndFunction

; E01B_Herd_BrahminTravelPackageRun runs while the scared keyword is present.
Function ApplyScaredKeyword(Float afDuration)
    brahminRef = GetActorReference()
    If brahminRef == None || brahminRef.IsDead() || E01B_Herd_ScaredBrahminKeyword == None
        Return
    EndIf
    brahminRef.AddKeyword(E01B_Herd_ScaredBrahminKeyword)
    brahminRef.EvaluatePackage()
    CancelTimer(4986)
    If afDuration > 0.0
        StartTimer(afDuration, 4986)
    EndIf
EndFunction

Function ClearScaredKeyword()
    CancelTimer(4986)
    Actor brahmin = GetActorReference()
    If brahmin != None && E01B_Herd_ScaredBrahminKeyword != None && brahmin.HasKeyword(E01B_Herd_ScaredBrahminKeyword)
        brahmin.RemoveKeyword(E01B_Herd_ScaredBrahminKeyword)
        brahmin.EvaluatePackage()
    EndIf
EndFunction

Function StartPanic()
    ApplyScaredKeyword(BrahminPanicDuration)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 4986
        ClearScaredKeyword()
    EndIf
EndEvent

Event OnDeath(Actor akKiller)
    CancelTimer(4986)
EndEvent

Event OnAliasShutdown()
    ClearScaredKeyword()
    If HerdingPerk != None
        UnregisterForRemoteEvent(HerdingPerk, "OnEntryRun")
    EndIf
    brahminRef = None
EndEvent
