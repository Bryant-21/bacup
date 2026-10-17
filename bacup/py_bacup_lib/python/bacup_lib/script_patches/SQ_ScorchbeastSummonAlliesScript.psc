; A scorchbeast's summon-allies screech (ScorchbeastSummonAlliesEffectScript) starts this quest with the scorchbeast as
; Ref1. Scorched within reach join ScorchedAllies and sandbox around it (package 346092); with fewer than MinAllies the
; Wave01 encounter wave (SpawnArea = Scorchbeast, LocEncScorched) brings more. The wave stops when the scorchbeast dies
; and the quest ends once no living ally is left near the player.
Event OnStoryScript(Keyword akKeyword, Location akLocation, ObjectReference akRef1, ObjectReference akRef2, Int aiValue1, Int aiValue2)
    Actor summoner = akRef1 as Actor
    If summoner == None
        summoner = akRef2 as Actor
    EndIf
    If summoner == None || summoner.IsDead()
        Stop()
        Return
    EndIf
    Scorchbeast.ForceRefTo(summoner)
    SQ_MasterIns = SQ_Master as sq_masterscript
    selfEWS = (Self as Quest) as defaultquestencounterwavescript
    RegisterForRemoteEvent(summoner, "OnDeath")
    If selfEWS != None && CountLivingAllies() < MinAllies
        selfEWS.StartEncounterWave(0)
    EndIf
    StartTimer(10.0, 1)
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If selfEWS != None
        selfEWS.StopEncounterWave(0, False)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Actor summoner = Scorchbeast.GetActorReference()
    Bool summonerActive = summoner != None && !summoner.IsDead() && summoner.Is3DLoaded()
    If !summonerActive && CountLivingAllies() == 0
        Stop()
        Return
    EndIf
    If summoner == None || !summoner.Is3DLoaded()
        Stop()
        Return
    EndIf
    StartTimer(10.0, 1)
EndEvent

Int Function CountLivingAllies()
    Int living = 0
    Int index = ScorchedAllies.GetCount() - 1
    While index >= 0
        Actor ally = ScorchedAllies.GetAt(index) as Actor
        If ally != None && !ally.IsDead() && ally.Is3DLoaded()
            living += 1
        EndIf
        index -= 1
    EndWhile
    Return living
EndFunction
