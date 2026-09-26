Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:sfs09:habitatquestscript
EndEvent

Int Function CountLivingMoleRats()
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor moleRat = GetAt(index) as Actor
        If moleRat != None && !moleRat.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

; Disturbing a sludge pile releases mole rats at its linked spawn point, capped at MaxNumMoleRats alive.
Function SpawnMoleRats(ObjectReference akSludgePile, Int aiCount)
    If akSludgePile == None || LvlMolerat == None || aiCount <= 0
        Return
    EndIf
    ObjectReference spawnPoint = None
    If SFS09_Habitat_MoleRatSpawnLinkedRef != None
        spawnPoint = akSludgePile.GetLinkedRef(SFS09_Habitat_MoleRatSpawnLinkedRef)
    EndIf
    If spawnPoint == None
        spawnPoint = akSludgePile
    EndIf
    Actor playerRef = Game.GetPlayer()
    Int spawned = 0
    While spawned < aiCount && CountLivingMoleRats() < MaxNumMoleRats
        Actor moleRat = spawnPoint.PlaceActorAtMe(LvlMolerat)
        If moleRat == None
            Return
        EndIf
        AddRef(moleRat)
        If playerRef != None
            moleRat.StartCombat(playerRef)
        EndIf
        spawned += 1
    EndWhile
EndFunction

Function RemoveMoleRats()
    Int index = GetCount() - 1
    While index >= 0
        ObjectReference moleRat = GetAt(index)
        If moleRat != None
            moleRat.Disable()
            moleRat.Delete()
        EndIf
        index -= 1
    EndWhile
    RemoveAll()
EndFunction

; Mole rats killed while the troughs are open can carry toxic sludge.
Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    If akSenderRef == None
        Return
    EndIf
    If QS == None
        OwningQuest = GetOwningQuest()
        QS = OwningQuest as Quests:sfs09:habitatquestscript
    EndIf
    If QS != None && SFS09_Habitat_Sludge != None && QS.IsTroughPhaseActive() && Utility.RandomFloat(0.0, 1.0) < QS.SludgeDropRate
        akSenderRef.AddItem(SFS09_Habitat_Sludge, 1, True)
    EndIf
    RemoveRef(akSenderRef)
    akSenderRef.Delete()
EndEvent
