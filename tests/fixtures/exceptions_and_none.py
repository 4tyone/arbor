"""Test file for exception and None source extraction."""

def simple_raise():
    raise ValueError("simple error")

def raise_with_condition(x):
    if x < 0:
        raise ValueError("must be positive")
    return x

def raise_from_exception():
    try:
        do_something()
    except Exception as e:
        raise RuntimeError("wrapped") from e

def raise_bare():
    try:
        risky()
    except:
        raise

def raise_class_no_args():
    raise KeyError

def raise_qualified():
    raise requests.exceptions.ConnectionError("failed")

def explicit_none_return():
    return None

def implicit_none_return():
    print("no return")

def conditional_none(x):
    if x:
        return x
    return None

def dict_get_none():
    d = {}
    return d.get("key")

def list_pop_none():
    items = []
    return items.pop() if items else None

def getattr_default():
    obj = object()
    return getattr(obj, "attr", None)

def call_other_function():
    result = some_function()
    other_function(result)
    obj.method()
    module.submodule.function()
    return result

class MyClass:
    def method_raise(self):
        raise NotImplementedError("subclass must implement")

    def method_call(self):
        self.helper()
        return self.get_value()
