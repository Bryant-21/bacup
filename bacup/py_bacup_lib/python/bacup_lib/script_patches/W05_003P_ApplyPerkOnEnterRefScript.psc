Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    bPlayerInside = True
    ApplyCurrentDiscount(akActionRef as Actor)
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        bPlayerInside = False
        RemoveSkinnerPerks(akActionRef as Actor)
    EndIf
EndEvent

Event OnLoad()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    If bPlayerInside
        ApplyCurrentDiscount(Game.GetPlayer())
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer() && bPlayerInside
        ApplyCurrentDiscount(akSender)
    EndIf
EndEvent

Event OnUnload()
    bPlayerInside = False
    RemoveSkinnerPerks(Game.GetPlayer())
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Function SetDiscountRank(Actor akPlayer, Int aiRank)
    If akPlayer != Game.GetPlayer() || W05_MQ_003P_Muscle_SkinnerPerkRank == None
        Return
    EndIf
    akPlayer.SetValue(W05_MQ_003P_Muscle_SkinnerPerkRank, aiRank)
    ApplyCurrentDiscount(akPlayer)
EndFunction

Function RemoveSkinnerPerks(Actor akPlayer)
    If akPlayer == None || SkinnerPerks == None
        Return
    EndIf
    Int i = 0
    While i < SkinnerPerks.Length
        If SkinnerPerks[i].PerkToApply != None && akPlayer.HasPerk(SkinnerPerks[i].PerkToApply)
            akPlayer.RemovePerk(SkinnerPerks[i].PerkToApply)
        EndIf
        i += 1
    EndWhile
EndFunction

Function ApplyCurrentDiscount(Actor akPlayer)
    If bProcessing || akPlayer == None || W05_MQ_003P_Muscle_SkinnerPerkRank == None || SkinnerPerks == None
        Return
    EndIf
    bProcessing = True
    RemoveSkinnerPerks(akPlayer)
    Int rank = akPlayer.GetValue(W05_MQ_003P_Muscle_SkinnerPerkRank) as Int
    Int found = SkinnerPerks.FindStruct("PerkIndex", rank)
    If found >= 0 && SkinnerPerks[found].PerkToApply
        akPlayer.AddPerk(SkinnerPerks[found].PerkToApply)
    EndIf
    bProcessing = False
EndFunction
