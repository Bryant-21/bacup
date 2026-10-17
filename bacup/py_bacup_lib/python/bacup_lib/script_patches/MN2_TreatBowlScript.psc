; One raid per bowl per run: TrickChance of a TreatExplosion trick, otherwise a TreatList treat.
Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player || !MischiefOpen() || !ClaimRef(akSenderRef)
        Return
    EndIf
    If TreatExplosion != None && Utility.RandomFloat(0.0, 1.0) < TrickChance
        akSenderRef.PlaceAtMe(TreatExplosion)
    ElseIf TreatList != None
        player.AddItem(TreatList)
    EndIf
    ScoreMischief(akSenderRef, player, 1.0)
EndEvent
