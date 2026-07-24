Event OnInit()
    Actor kWanderer = Wanderer.GetReference() as Actor
    If kWanderer == None
        Return
    EndIf
    RegisterForRemoteEvent(kWanderer, "OnCombatStateChanged")
    RegisterForRemoteEvent(kWanderer, "OnDeath")
EndEvent

Event Actor.OnDeath(Actor akVictim, Actor akKiller)
    If W05_RE_TravelBB01_DeadScene != None
        W05_RE_TravelBB01_DeadScene.Start()
    EndIf
EndEvent

Event Actor.OnCombatStateChanged(Actor akSender, Actor akTarget, int aeCombatState)
    If aeCombatState == 2
        If W05_RE_TravelBB01_InCombatScene != None
            W05_RE_TravelBB01_InCombatScene.Start()
        EndIf
    ElseIf aeCombatState == 1
        If W05_RE_TravelBB01_SearchingScene != None
            W05_RE_TravelBB01_SearchingScene.Start()
        EndIf
    Else
        If W05_RE_TravelBB01_NotInCombatScene != None
            W05_RE_TravelBB01_NotInCombatScene.Start()
        EndIf
    EndIf
EndEvent
