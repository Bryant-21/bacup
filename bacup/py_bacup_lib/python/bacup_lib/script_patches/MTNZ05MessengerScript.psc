; Mr. Messenger is repaired when the player hands over the part (RestoreStage 210): his health is restored fully.
Event OnAliasInit()
    RegisterForRemoteEvent(GetOwningQuest(), "OnStageSet")
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID != RestoreStage
        Return
    EndIf
    Actor messenger = GetActorReference()
    If messenger != None && !messenger.IsDead()
        fHealth = messenger.GetBaseValue(Health)
        messenger.RestoreValue(Health, fHealth + fExtraDMG)
    EndIf
EndEvent
