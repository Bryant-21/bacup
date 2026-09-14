Event OnDeath(Actor akKiller)
    If akKiller == None || PlayerAlias == None
        Return
    EndIf

    Actor kPlayer = PlayerAlias.GetActorReference()
    Quest kQuest = GetOwningQuest()
    If kPlayer != None && akKiller == kPlayer && kQuest != None
        kQuest.SetStage(100)
    EndIf
EndEvent
