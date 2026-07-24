Event OnTriggerEnter(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && Game.GetPlayer().GetValue(W05_MQ_002P_Radical_HeardRoperWarning) < 1.0 && !W05_MQ_002P_Radical.IsCompleted()
        Loudspeaker.GetReference().Say(W05_002P_RoperWarning)
        Game.GetPlayer().SetValue(W05_MQ_002P_Radical_HeardRoperWarning, 1.0)
    EndIf
EndEvent
