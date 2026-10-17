Function Fragment_Phase_01_End()
    GQ_SuperMutantMerchantScript route = GetOwningQuest() as GQ_SuperMutantMerchantScript
    If route != None
        route.ArriveAtStop()
    EndIf
EndFunction
