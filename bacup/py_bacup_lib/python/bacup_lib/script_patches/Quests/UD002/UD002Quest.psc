Event OnQuestInit()
    EWS = (Self as Quest) as defaultquestencounterwavescript
    ReferenceAlias bossFurnitureAlias = GetAlias(14) as ReferenceAlias
    ObjectReference bossFurniture
    If bossFurnitureAlias != None
        bossFurniture = bossFurnitureAlias.GetReference()
    EndIf
    Actor existingBoss = Boss.GetReference() as Actor
    If existingBoss != None
        SpawnedBossActor = existingBoss
    Else
        If bossFurniture != None && Assaultron != None
            SpawnedBossActor = bossFurniture.PlaceActorAtMe(Assaultron, 3)
            If SpawnedBossActor != None
                Boss.ForceRefTo(SpawnedBossActor)
            EndIf
        EndIf
    EndIf

    If SpawnedBossActor != None
        If BossCollection != None && BossCollection.Find(SpawnedBossActor) < 0
            BossCollection.AddRef(SpawnedBossActor)
        EndIf
        If bossFurniture != None && LinkAmbushFurniture != None
            SpawnedBossActor.SetLinkedRef(bossFurniture, LinkAmbushFurniture)
        EndIf
        If AmbushRelease != None
            SpawnedBossActor.SetValue(AmbushRelease, 1.0)
        EndIf
        SpawnedBossActor.Enable(False)
        Actor playerRef = Game.GetPlayer()
        If playerRef != None && !SpawnedBossActor.IsDead()
            SpawnedBossActor.StartCombat(playerRef)
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    If SpawnedBossActor != None && !SpawnedBossActor.IsDead()
        SpawnedBossActor.StopCombat()
        SpawnedBossActor.Disable(False)
        SpawnedBossActor.Delete()
    EndIf
    SpawnedBossActor = None
    EWS = None
EndEvent
