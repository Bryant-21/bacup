Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && ((W05_MQ_002P_Radical_PlayerKilledRoper != None && Game.GetPlayer().GetValue(W05_MQ_002P_Radical_PlayerKilledRoper) >= 1.0) || Game.GetPlayer().GetValue(W05_MQ_002P_Radical_RadicalFriendValue) < 1.0)
        Game.GetPlayer().AddToFaction(W05_RadicalEnemyFaction)
    EndIf
EndEvent
