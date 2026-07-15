; Offline exchange: one pre-war money buys the configured token count. FO76
; normally computes the count server-side, so an unbound/zero count safely falls
; back to one token without inventing a Luck formula.

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If akActionRef.GetItemCount(PreWarMoneyForm) < 1
        If NotEnoughMoneyMessage != None
            NotEnoughMoneyMessage.Show()
        EndIf
        Return
    EndIf

    Int tokenCount = TokensToDispense
    If tokenCount <= 0
        tokenCount = 1
    EndIf
    PlayInsertSound()
    akActionRef.RemoveItem(PreWarMoneyForm, 1, True)
    akActionRef.AddItem(NukacadeTokenForm, tokenCount, False)
    PlayDispenseSound(tokenCount)
EndEvent

Function PlayInsertSound()
    If TokenExchangeInsertSFX != None
        TokenExchangeInsertSFX.Play(Self)
    EndIf
EndFunction

Function PlayDispenseSound(Int aiTokensToGive)
    Int i = 0
    While i < aiTokensToGive
        If TokenExchangeDispenseSFX != None
            TokenExchangeDispenseSFX.Play(Self)
        EndIf
        Utility.Wait(0.1)
        i = i + 1
    EndWhile
EndFunction
